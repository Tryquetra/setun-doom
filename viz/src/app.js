import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

// Exact H16 measured hardware telemetry from GW2AR-LV18QN88C8/I7 + iverilog (6,000 cycles x 8 lanes)
const H16_DATA = [
  { key: "hexgray3b", name: "HexGray3 (Q3)", wires: 24, dff: 24, lut4: 32, lut1: 0, mux2: 24, fmax: 328.30, toggles: 48784, glitches: 782, color: 0x34d399, winner: true },
  { key: "canon6b", name: "Canonical 2b/trit", wires: 48, dff: 48, lut4: 128, lut1: 384, mux2: 384, fmax: 217.30, toggles: 96000, glitches: 48000, color: 0x38bdf8, winner: false },
  { key: "offset6b", name: "Offset 2b/trit", wires: 48, dff: 48, lut4: 160, lut1: 288, mux2: 384, fmax: 241.14, toggles: 144004, glitches: 48000, color: 0xdca93e, winner: false },
  { key: "onehot7b", name: "One-Hot 7-state", wires: 56, dff: 56, lut4: 186, lut1: 262, mux2: 216, fmax: 190.01, toggles: 96000, glitches: 48000, color: 0xa78bfa, winner: false },
  { key: "onehot9b", name: "One-Hot 3b/trit", wires: 72, dff: 72, lut4: 214, lut1: 549, mux2: 391, fmax: 195.50, toggles: 192000, glitches: 48000, color: 0xff4d6d, winner: false },
];

// Geometric zero-sum dictionary (q+r+s=0) + fault sentinel
const HEX_STATES = {
  center: { code: 0b000, label: "Center (0°)", qrs: [0, 0, 0], angle: null, linearVal: 1 },
  v0: { code: 0b001, label: "V0 (0°)", qrs: [1, 0, -1], angle: 0, linearVal: 2 },
  v1: { code: 0b011, label: "V1 (60°)", qrs: [0, 1, -1], angle: Math.PI / 3, linearVal: 3 },
  v2: { code: 0b010, label: "V2 (120°)", qrs: [-1, 1, 0], angle: (2 * Math.PI) / 3, linearVal: 0 },
  v3: { code: 0b110, label: "V3 (180°)", qrs: [-1, 0, 1], angle: Math.PI, linearVal: -1 },
  v4: { code: 0b111, label: "V4 (240°)", qrs: [0, -1, 1], angle: (4 * Math.PI) / 3, linearVal: -2 },
  v5: { code: 0b101, label: "V5 (300°)", qrs: [1, -1, 0], angle: (5 * Math.PI) / 3, linearVal: -3 },
  sentinel: { code: 0b100, label: "Sentinel Fault", qrs: null, angle: null, linearVal: null },
};

const RING_KEYS = ["v0", "v1", "v2", "v3", "v4", "v5"];
const LINEAR_KEYS = ["v5", "v4", "v3", "v2", "center", "v0", "v1"]; // -3 .. +3 (d_H = 1 everywhere)

function tritToCanonBits(t) {
  return t === 0 ? [0, 0] : t === 1 ? [0, 1] : [1, 0];
}
function tritToOffsetBits(t) {
  return t === -1 ? [0, 0] : t === 0 ? [0, 1] : [1, 0];
}
function tritToOneHotBits(t) {
  return t === 0 ? [0, 0, 1] : t === 1 ? [0, 1, 0] : [1, 0, 0];
}

function encodeAllForKey(key, mode, linearIndex) {
  const st = HEX_STATES[key];
  if (key === "sentinel") {
    return {
      hexgray3b: [1, 0, 0],
      canon6b: [1, 1, 1, 1, 1, 1],
      offset6b: [1, 1, 1, 1, 1, 1],
      onehot7b: [0, 0, 0, 0, 0, 0, 0],
      onehot9b: [0, 0, 0, 0, 0, 0, 0, 0, 0],
      tritsForSwing: [0, 0, 0],
    };
  }
  const [q, r, s] = st.qrs;
  const ringOrCenterIdx = key === "center" ? 0 : RING_KEYS.indexOf(key) + 1;
  const onehot7b = Array.from({ length: 7 }, (_, i) => (i === ringOrCenterIdx ? 1 : 0));

  // In linear mode [-3..+3], show the 2-trit balanced canonical integer on Canon/Offset/OneHot trits
  // so user sees the real carry snap at +1 <-> +2 (and -1 <-> -2), while ring mode shows (q, r, s).
  let displayTrits = [q, r, s];
  if (mode === "linear") {
    const val = linearIndex - 3; // -3..+3
    const rem = ((val % 3) + 3) % 3;
    const t0 = rem === 0 ? 0 : rem === 1 ? 1 : -1;
    const t1 = (val - t0) / 3;
    displayTrits = [t0, t1, 0];
  }

  return {
    hexgray3b: [(st.code >> 2) & 1, (st.code >> 1) & 1, st.code & 1],
    canon6b: [...tritToCanonBits(displayTrits[2]), ...tritToCanonBits(displayTrits[1]), ...tritToCanonBits(displayTrits[0])],
    offset6b: [...tritToOffsetBits(displayTrits[2]), ...tritToOffsetBits(displayTrits[1]), ...tritToOffsetBits(displayTrits[0])],
    onehot7b,
    onehot9b: [...tritToOneHotBits(displayTrits[2]), ...tritToOneHotBits(displayTrits[1]), ...tritToOneHotBits(displayTrits[0])],
    tritsForSwing: displayTrits,
  };
}

function popDiff(a, b) {
  let mask = [];
  let count = 0;
  for (let i = 0; i < a.length; i++) {
    const ch = a[i] !== b[i] ? 1 : 0;
    mask.push(ch);
    count += ch;
  }
  return { mask, count };
}

function swingStats(a, b) {
  let half = 0;
  let full = 0;
  for (let i = 0; i < a.length; i++) {
    const d = Math.abs(a[i] - b[i]);
    if (d === 1) half += 1;
    else if (d === 2) full += 1;
  }
  return { half, full };
}

const CODING_SPECS = [
  { id: "hexgray3b", title: "HexGray3", subtitle: "3 wires | Q3 Gray", color: 0x34d399, x: -5.4 },
  { id: "canon6b", title: "Canonical 2b", subtitle: "6 wires | 00/01/10", color: 0x38bdf8, x: -2.7 },
  { id: "offset6b", title: "Offset 2b", subtitle: "6 wires | -1=00,0=01,+1=10", color: 0xdca93e, x: 0.0 },
  { id: "onehot7b", title: "One-Hot 7", subtitle: "7 wires | 1 per state", color: 0xa78bfa, x: 2.7 },
  { id: "onehot9b", title: "One-Hot 3b", subtitle: "9 wires | H15 LUT win", color: 0xff4d6d, x: 5.4 },
];

const appState = {
  activeTab: "overview",
  mode: "ring",
  ringPos: 0, // index into RING_KEYS or -1 for center, -2 for sentinel
  linearIdx: 4, // 0..6 in LINEAR_KEYS
  stateKey: "v0",
  prevKey: "v0",
  prevBits: null,
  lastMasks: {},
  lastDeltas: {},
  totals: {
    steps: 0,
    halfSwings: 0,
    fullSwings: 0,
    flips: { hexgray3b: 0, canon6b: 0, offset6b: 0, onehot7b: 0, onehot9b: 0 },
    glitches: { hexgray3b: 0, canon6b: 0, offset6b: 0, onehot7b: 0, onehot9b: 0 },
  },
  autoPlay: false,
  autoAccum: 0,
  pulseTimer: 0,
  skewNorm: 0.65,
  selectedH16: "hexgray3b",
  reducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
};

appState.prevBits = encodeAllForKey("v0", "ring", 4);
for (const s of CODING_SPECS) {
  appState.lastMasks[s.id] = new Array(appState.prevBits[s.id].length).fill(0);
  appState.lastDeltas[s.id] = 0;
}

// ---------- Three.js Setup ----------
const stageEl = document.getElementById("stage");
const scene = new THREE.Scene();
scene.background = new THREE.Color(0x070b14);
scene.fog = new THREE.FogExp2(0x070b14, 0.022);

const camera = new THREE.PerspectiveCamera(42, stageEl.clientWidth / Math.max(1, stageEl.clientHeight), 0.1, 120);
camera.position.set(0, 4.4, 12.6);

const renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
renderer.setSize(stageEl.clientWidth, Math.max(1, stageEl.clientHeight));
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
renderer.toneMapping = THREE.ACESFilmicToneMapping;
renderer.toneMappingExposure = 1.1;
stageEl.appendChild(renderer.domElement);

const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;
controls.dampingFactor = 0.06;
controls.target.set(0, 0.2, 0);

scene.add(new THREE.AmbientLight(0x94a3b8, 0.95));
const dirKey = new THREE.DirectionalLight(0xfff7e6, 2.2);
dirKey.position.set(6, 12, 9);
scene.add(dirKey);
const dirRim = new THREE.DirectionalLight(0x38bdf8, 1.1);
dirRim.position.set(-8, 6, -6);
scene.add(dirRim);

const grid = new THREE.GridHelper(26, 26, 0x1e293b, 0x0f172a);
grid.position.y = -1.6;
scene.add(grid);

function makeLabelSprite(text, colorCss, scaleHeight = 0.34) {
  const canvas = document.createElement("canvas");
  const fontSize = text.length > 10 ? 44 : 56;
  const font = `700 ${fontSize}px ui-monospace, SFMono-Regular, Menlo, monospace`;
  const tmp = canvas.getContext("2d");
  tmp.font = font;
  const w = Math.ceil(tmp.measureText(text).width + 44);
  canvas.width = Math.max(160, w);
  canvas.height = 110;
  const ctx = canvas.getContext("2d");
  ctx.font = font;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = colorCss;
  ctx.fillText(text, canvas.width / 2, canvas.height / 2);
  const tex = new THREE.CanvasTexture(canvas);
  const mat = new THREE.SpriteMaterial({ map: tex, transparent: true, depthWrite: false });
  const sp = new THREE.Sprite(mat);
  sp.scale.set((canvas.width / canvas.height) * scaleHeight, scaleHeight, 1);
  return sp;
}

// ======================================================
// VIEW 1: OVERVIEW ARENA (5 Side-by-Side Codings)
// ======================================================
const overviewGroup = new THREE.Group();
scene.add(overviewGroup);
const overviewViews = {};

for (const spec of CODING_SPECS) {
  const grp = new THREE.Group();
  grp.position.set(spec.x, 0, 0);
  overviewGroup.add(grp);

  const base = new THREE.Mesh(
    new THREE.CylinderGeometry(1.02, 1.12, 0.24, 6),
    new THREE.MeshStandardMaterial({ color: 0x111b2e, metalness: 0.35, roughness: 0.45 })
  );
  base.position.y = -0.3;
  grp.add(base);

  const rim = new THREE.Mesh(
    new THREE.RingGeometry(0.95, 1.03, 6),
    new THREE.MeshBasicMaterial({ color: spec.color, side: THREE.DoubleSide })
  );
  rim.rotation.x = -Math.PI / 2;
  rim.rotation.z = Math.PI / 6;
  rim.position.y = -0.17;
  grp.add(rim);

  const rotor = new THREE.Group();
  rotor.position.y = 0.02;
  grp.add(rotor);

  const arrowShaft = new THREE.Mesh(
    new THREE.BoxGeometry(0.55, 0.12, 0.14),
    new THREE.MeshStandardMaterial({ color: spec.color, emissive: spec.color, emissiveIntensity: 0.45 })
  );
  arrowShaft.position.x = 0.25;
  rotor.add(arrowShaft);

  const arrowHead = new THREE.Mesh(
    new THREE.ConeGeometry(0.16, 0.3, 6),
    new THREE.MeshStandardMaterial({ color: spec.color, emissive: spec.color, emissiveIntensity: 0.55 })
  );
  arrowHead.rotation.z = -Math.PI / 2;
  arrowHead.position.x = 0.62;
  rotor.add(arrowHead);

  const centerHub = new THREE.Mesh(
    new THREE.SphereGeometry(0.24, 24, 18),
    new THREE.MeshStandardMaterial({ color: 0xf8fafc, emissive: spec.color, emissiveIntensity: 0.35 })
  );
  grp.add(centerHub);

  // Skew warning halo above pedestal when d_H >= 2
  const hazardRing = new THREE.Mesh(
    new THREE.TorusGeometry(0.74, 0.045, 12, 32),
    new THREE.MeshBasicMaterial({ color: 0xff4d6d, transparent: true, opacity: 0.0 })
  );
  hazardRing.rotation.x = Math.PI / 2;
  hazardRing.position.y = 0.72;
  grp.add(hazardRing);

  // Physical wire bars (3, 6, 6, 7, 9 wires)
  const bitCount = encodeAllForKey("v0", "ring", 4)[spec.id].length;
  const wireBars = [];
  const barSpan = 1.95;
  const stepX = barSpan / Math.max(1, bitCount - 1);
  for (let i = 0; i < bitCount; i++) {
    const bar = new THREE.Mesh(
      new THREE.BoxGeometry(bitCount > 7 ? 0.14 : 0.19, 0.45, 0.22),
      new THREE.MeshStandardMaterial({ color: 0x1e293b, emissive: 0x000000, emissiveIntensity: 0.1 })
    );
    bar.position.set(-barSpan / 2 + i * stepX, -0.92, 0.85);
    grp.add(bar);
    wireBars.push(bar);
  }

  const titleSp = makeLabelSprite(spec.title, "#f8fafc", 0.34);
  titleSp.position.set(0, 1.35, 0);
  grp.add(titleSp);

  const subSp = makeLabelSprite(spec.subtitle, "#94a3b8", 0.22);
  subSp.position.set(0, 1.0, 0);
  grp.add(subSp);

  overviewViews[spec.id] = { grp, rotor, hazardRing, wireBars, arrowShaft, arrowHead, centerHub, spec };
}

// ======================================================
// VIEW 2: GEOMETRY (3D Q3 Cube vs Zero-Sum Hex Rosette)
// ======================================================
const geometryGroup = new THREE.Group();
scene.add(geometryGroup);
geometryGroup.visible = false;

const CUBE_COORDS = {
  0b000: [-1.15, -1.15, -1.15],
  0b001: [1.15, -1.15, -1.15],
  0b010: [-1.15, 1.15, -1.15],
  0b011: [1.15, 1.15, -1.15],
  0b100: [-1.15, -1.15, 1.15],
  0b101: [1.15, -1.15, 1.15],
  0b110: [-1.15, 1.15, 1.15],
  0b111: [1.15, 1.15, 1.15],
};

const cubeRig = new THREE.Group();
cubeRig.position.set(-3.1, 0.15, 0);
geometryGroup.add(cubeRig);

const cubeEdgePairs = [
  [0, 1], [1, 3], [3, 2], [2, 0],
  [4, 5], [5, 7], [7, 6], [6, 4],
  [0, 4], [1, 5], [2, 6], [3, 7],
];
const C6_CYCLE_EDGES = new Set(["1-3", "2-3", "2-6", "6-7", "5-7", "1-5"]);

for (const [a, b] of cubeEdgePairs) {
  const edgeKey = `${Math.min(a, b)}-${Math.max(a, b)}`;
  const isCycle = C6_CYCLE_EDGES.has(edgeKey);
  const touchesFault = a === 4 || b === 4;
  const col = isCycle ? 0xdca93e : touchesFault ? 0xff4d6d : 0x334155;
  const pts = [new THREE.Vector3(...CUBE_COORDS[a]), new THREE.Vector3(...CUBE_COORDS[b])];
  const line = new THREE.Line(
    new THREE.BufferGeometry().setFromPoints(pts),
    new THREE.LineBasicMaterial({ color: col })
  );
  cubeRig.add(line);
}

const pickables = [];
const cubeNodes = {};
const codeToKey = Object.fromEntries(Object.entries(HEX_STATES).map(([k, v]) => [v.code, k]));

for (let code = 0; code < 8; code++) {
  const pos = CUBE_COORDS[code];
  const key = codeToKey[code];
  const color = code === 0 ? 0x34d399 : code === 0b100 ? 0xff4d6d : 0x38bdf8;
  const sphere = new THREE.Mesh(
    new THREE.SphereGeometry(0.18, 22, 16),
    new THREE.MeshStandardMaterial({ color, emissive: color, emissiveIntensity: 0.35 })
  );
  sphere.position.set(...pos);
  sphere.userData = { targetKey: key };
  cubeRig.add(sphere);
  pickables.push(sphere);
  cubeNodes[code] = sphere;

  const lbl = makeLabelSprite(code.toString(2).padStart(3, "0"), code === 0b100 ? "#fecdd3" : "#e2e8f0", 0.25);
  lbl.position.set(pos[0] * 1.24, pos[1] * 1.24, pos[2] * 1.24);
  cubeRig.add(lbl);
}

const cubeActiveBead = new THREE.Mesh(
  new THREE.SphereGeometry(0.28, 26, 20),
  new THREE.MeshStandardMaterial({ color: 0xfef08a, emissive: 0xdca93e, emissiveIntensity: 0.9, wireframe: true })
);
cubeRig.add(cubeActiveBead);

const cubeTitle = makeLabelSprite("Boolean Hypercube Q3 (3 Wires)", "#f8fafc", 0.32);
cubeTitle.position.set(0, 2.15, 0);
cubeRig.add(cubeTitle);

// Right stage: Hexagonal Plane q+r+s=0
const hexRig = new THREE.Group();
hexRig.position.set(3.0, 0.0, 0);
geometryGroup.add(hexRig);

const hexTitle = makeLabelSprite("Hex Plane (q+r+s=0, 7 Valid States)", "#f8fafc", 0.32);
hexTitle.position.set(0, 2.15, 0);
hexRig.add(hexTitle);

const hexTiles = {};
for (const [key, st] of Object.entries(HEX_STATES)) {
  if (key === "sentinel") continue;
  const x = st.angle === null ? 0 : Math.cos(st.angle) * 1.7;
  const z = st.angle === null ? 0 : -Math.sin(st.angle) * 1.7;
  const col = key === "center" ? 0x34d399 : 0x38bdf8;
  const tile = new THREE.Mesh(
    new THREE.CylinderGeometry(0.56, 0.56, 0.16, 6),
    new THREE.MeshStandardMaterial({ color: 0x0f172a, emissive: col, emissiveIntensity: 0.2 })
  );
  tile.position.set(x, -0.35, z);
  tile.userData = { targetKey: key };
  hexRig.add(tile);
  pickables.push(tile);
  hexTiles[key] = tile;

  const tLbl = makeLabelSprite(`(${st.qrs.join(",")})`, "#e2e8f0", 0.22);
  tLbl.position.set(x, 0.05, z);
  hexRig.add(tLbl);
}

const hexActiveBeacon = new THREE.Mesh(
  new THREE.TorusGeometry(0.62, 0.06, 14, 32),
  new THREE.MeshStandardMaterial({ color: 0xdca93e, emissive: 0xdca93e, emissiveIntensity: 0.8 })
);
hexActiveBeacon.rotation.x = Math.PI / 2;
hexRig.add(hexActiveBeacon);

// ======================================================
// VIEW 3: SKEW GLITCH & SWING ENERGY LAB
// ======================================================
const skewGroup = new THREE.Group();
scene.add(skewGroup);
skewGroup.visible = false;

const skewChannelSpecs = [
  { id: "hexgray3b", title: "HexGray3 (3 wires)", y: 1.55, wires: 3, color: 0x34d399 },
  { id: "canon6b", title: "Canonical 2b (6 wires)", y: -0.05, wires: 6, color: 0x38bdf8 },
  { id: "onehot9b", title: "One-Hot 3b (9 wires)", y: -1.65, wires: 9, color: 0xff4d6d },
];

const skewLanes = {};
for (const lane of skewChannelSpecs) {
  const grp = new THREE.Group();
  grp.position.set(0, lane.y, 0);
  skewGroup.add(grp);

  const txBox = new THREE.Mesh(
    new THREE.BoxGeometry(0.7, 0.9, 0.7),
    new THREE.MeshStandardMaterial({ color: 0x1e293b, emissive: lane.color, emissiveIntensity: 0.25 })
  );
  txBox.position.x = -5.0;
  grp.add(txBox);

  const rxBox = new THREE.Mesh(
    new THREE.BoxGeometry(0.75, 0.95, 0.75),
    new THREE.MeshStandardMaterial({ color: 0x1e293b, emissive: lane.color, emissiveIntensity: 0.25 })
  );
  rxBox.position.x = 5.0;
  grp.add(rxBox);

  const lbl = makeLabelSprite(lane.title, "#f8fafc", 0.28);
  lbl.position.set(-5.0, 0.72, 0);
  grp.add(lbl);

  const traces = [];
  const pulses = [];
  const zSpread = lane.wires <= 3 ? 0.5 : 1.05;
  for (let w = 0; w < lane.wires; w++) {
    const zPos = lane.wires === 1 ? 0 : -zSpread / 2 + (w * zSpread) / (lane.wires - 1);
    const wire = new THREE.Mesh(
      new THREE.CylinderGeometry(0.025, 0.025, 9.4, 8),
      new THREE.MeshStandardMaterial({ color: 0x334155 })
    );
    wire.rotation.z = Math.PI / 2;
    wire.position.set(0, 0, zPos);
    grp.add(wire);
    traces.push(wire);

    const pulse = new THREE.Mesh(
      new THREE.SphereGeometry(0.11, 14, 12),
      new THREE.MeshStandardMaterial({ color: lane.color, emissive: lane.color, emissiveIntensity: 0.9 })
    );
    pulse.position.set(-4.6, 0, zPos);
    pulse.visible = false;
    grp.add(pulse);
    pulses.push(pulse);
  }

  skewLanes[lane.id] = { grp, txBox, rxBox, traces, pulses, lane };
}

// ======================================================
// VIEW 4: TANG NANO 20K SILICON SKYLINE (H16 Measurements)
// ======================================================
const siliconGroup = new THREE.Group();
scene.add(siliconGroup);
siliconGroup.visible = false;

const maxToggles = 192000;
const maxLuts = 763;
const maxDff = 72;
const maxFmax = 328.3;
const metricColors = [0x38bdf8, 0xa78bfa, 0xdca93e, 0x34d399];
const metricNames = ["Toggles", "LUTs", "DFFs", "Fmax"];

const siliconClusters = [];
for (let idx = 0; idx < H16_DATA.length; idx++) {
  const row = H16_DATA[idx];
  const cx = -5.2 + idx * 2.6;
  const cluster = new THREE.Group();
  cluster.position.set(cx, -1.5, 0);
  siliconGroup.add(cluster);

  const vals = [
    row.toggles / maxToggles,
    (row.lut4 + row.lut1) / maxLuts,
    row.dff / maxDff,
    row.fmax / maxFmax,
  ];

  const bars = [];
  for (let m = 0; m < 4; m++) {
    const h = Math.max(0.16, vals[m] * 2.55);
    const mesh = new THREE.Mesh(
      new THREE.BoxGeometry(0.4, h, 0.4),
      new THREE.MeshStandardMaterial({
        color: metricColors[m],
        emissive: metricColors[m],
        emissiveIntensity: row.winner ? 0.35 : 0.14,
      })
    );
    mesh.position.set(-0.72 + m * 0.48, h / 2, 0);
    mesh.userData = { h16Key: row.key };
    cluster.add(mesh);
    pickables.push(mesh);
    bars.push(mesh);
  }

  const plate = new THREE.Mesh(
    new THREE.BoxGeometry(2.15, 0.08, 0.9),
    new THREE.MeshStandardMaterial({ color: row.winner ? 0x064e3b : 0x1e293b })
  );
  plate.position.y = -0.04;
  plate.userData = { h16Key: row.key };
  cluster.add(plate);
  pickables.push(plate);

  const nameLbl = makeLabelSprite(row.name, row.winner ? "#a7f3d0" : "#e2e8f0", 0.32);
  nameLbl.position.set(0, -0.4, 0.72);
  cluster.add(nameLbl);

  siliconClusters.push({ key: row.key, cluster, bars, plate });
}

// Legend sprite strip in Silicon skyline
for (let m = 0; m < 4; m++) {
  const leg = makeLabelSprite(metricNames[m], "#" + metricColors[m].toString(16).padStart(6, "0"), 0.32);
  leg.position.set(-3.0 + m * 2.0, 1.85, 0);
  siliconGroup.add(leg);
}

// ======================================================
// STATE STEPPING & UI BINDING
// ======================================================
function transitionTo(nextKey) {
  const prevBits = appState.prevBits;
  const nextBits = encodeAllForKey(nextKey, appState.mode, appState.linearIdx);

  const sw = swingStats(prevBits.tritsForSwing, nextBits.tritsForSwing);
  if (nextKey !== appState.stateKey) {
    appState.totals.steps += 1;
    appState.totals.halfSwings += sw.half;
    appState.totals.fullSwings += sw.full;
  }

  for (const spec of CODING_SPECS) {
    const { mask, count } = popDiff(prevBits[spec.id], nextBits[spec.id]);
    appState.lastMasks[spec.id] = mask;
    appState.lastDeltas[spec.id] = count;
    if (nextKey !== appState.stateKey) {
      appState.totals.flips[spec.id] += count;
      if (count >= 2) appState.totals.glitches[spec.id] += 1;
    }
  }

  appState.prevKey = appState.stateKey;
  appState.stateKey = nextKey;
  appState.prevBits = nextBits;
  appState.pulseTimer = appState.reducedMotion ? 0.2 : 1.0;

  updateUI(sw);
}

function stepDirection(delta) {
  if (appState.mode === "ring") {
    if (appState.stateKey === "center" || appState.stateKey === "sentinel") {
      appState.ringPos = delta > 0 ? 0 : 2;
    } else {
      appState.ringPos = (appState.ringPos + delta + 6) % 6;
    }
    transitionTo(RING_KEYS[appState.ringPos]);
  } else {
    appState.linearIdx = Math.max(0, Math.min(LINEAR_KEYS.length - 1, appState.linearIdx + delta));
    transitionTo(LINEAR_KEYS[appState.linearIdx]);
  }
}

function updateUI(lastSwing = { half: 0, full: 0 }) {
  const st = HEX_STATES[appState.stateKey];
  document.getElementById("rd-label").textContent =
    appState.mode === "linear" && st.linearVal !== null ? `Level ${st.linearVal >= 0 ? "+" + st.linearVal : st.linearVal} (${st.label})` : st.label;
  document.getElementById("rd-code").textContent = `3'b${st.code.toString(2).padStart(3, "0")}`;
  document.getElementById("rd-qrs").textContent = st.qrs ? `(${st.qrs.join(", ")})` : "FAULT";
  document.getElementById("rd-swing").textContent =
    `${lastSwing.half} half / ${lastSwing.full} full (total: ${appState.totals.halfSwings}h / ${appState.totals.fullSwings}f)`;

  // Overview cards
  const cardsEl = document.getElementById("overview-cards");
  cardsEl.innerHTML = CODING_SPECS.map((s) => {
    const last = appState.lastDeltas[s.id];
    const total = appState.totals.flips[s.id];
    const glitch = appState.totals.glitches[s.id];
    const cls = s.id === "hexgray3b" ? "info-card win" : last >= 2 ? "info-card warn" : "info-card";
    const metricCls = s.id === "hexgray3b" ? "metric-big" : glitch > 0 ? "metric-big red" : "metric-big amber";
    const bitsStr = appState.prevBits[s.id].join("");
    return `
      <div class="${cls}">
        <h3>${s.title}</h3>
        <div class="${metricCls}">${total} flips | ${glitch} glitches</div>
        <p>Bits: <code>${bitsStr}</code><br>Last step: <strong>${last} wire(s)</strong> flipped</p>
      </div>
    `;
  }).join("");

  // Skew cards
  const hgD = appState.lastDeltas.hexgray3b;
  const canD = appState.lastDeltas.canon6b;
  document.getElementById("skew-hg3-status").textContent =
    hgD <= 1 ? `CLEAN (${hgD} wire flipped, 0 torn states)` : `RESET JUMP (${hgD} wires)`;
  document.getElementById("skew-can-status").textContent =
    canD >= 2 ? `HAZARD (${canD} wires in flight, torn intermediate word)` : `${canD} wire changed`;

  // H16 Table highlight
  renderH16Table();
}

function renderH16Table() {
  const tbody = document.getElementById("h16-tbody");
  tbody.innerHTML = H16_DATA.map((r) => {
    const classes = [r.winner ? "is-winner" : "", appState.selectedH16 === r.key ? "is-selected" : ""].filter(Boolean).join(" ");
    return `
      <tr class="${classes}" data-key="${r.key}">
        <th>${r.name}</th>
        <td data-label="Wires / DFF">${r.wires}</td>
        <td data-label="LUT4">${r.lut4}</td>
        <td data-label="LUT1">${r.lut1}</td>
        <td data-label="MUX2_LUT5">${r.mux2}</td>
        <td data-label="Fmax (MHz)">${r.fmax.toFixed(2)}</td>
        <td data-label="Toggles (6k)">${r.toggles.toLocaleString("en-US")}</td>
        <td data-label="Glitch Cycles">${r.glitches.toLocaleString("en-US")}</td>
      </tr>
    `;
  }).join("");
}

// Accessible Tabs
const tablist = document.getElementById("main-tablist");
const tabs = Array.from(tablist.querySelectorAll('[role="tab"]'));
const tabToPanel = {
  "tab-overview": ["panel-overview", overviewGroup, [0, 4.4, 12.6], [0, -0.1, 0], "View 1: Overview Arena (All 5 Encodings Live)"],
  "tab-geometry": ["panel-geometry", geometryGroup, [0, 3.3, 10.6], [0, 0.0, 0], "View 2: Hypercube Q3 vs Hexagonal Plane (Click vertices)"],
  "tab-skew": ["panel-skew", skewGroup, [0, 2.2, 11.4], [0, 0.0, 0], "View 3: Physical Wire Propagation Skew & Torn-Word Glitch Lab"],
  "tab-silicon": ["panel-silicon", siliconGroup, [0, 3.3, 15.2], [0, -0.15, 0], "View 4: Tang Nano 20K H16 Silicon Skyline (Click bar clusters)"],
};

function selectTab(tab) {
  for (const t of tabs) {
    const active = t === tab;
    t.setAttribute("aria-selected", String(active));
    t.toggleAttribute("focusgroupstart", active);
    const [panelId, group] = tabToPanel[t.id];
    const panel = document.getElementById(panelId);
    panel.hidden = !active;
    if (active) {
      if (panelId === "panel-overview" || panelId === "panel-geometry") panel.setAttribute("tabindex", "0");
    } else {
      panel.removeAttribute("tabindex");
    }
    group.visible = active;
  }
  appState.activeTab = tab.id;
  const [, , camPos, tgt, pill] = tabToPanel[tab.id];
  camera.position.set(...camPos);
  controls.target.set(...tgt);
  controls.update();
  document.getElementById("stage-pill").textContent = `${pill} | Drag to orbit, scroll to zoom`;
}

tabs.forEach((t, idx) => {
  t.addEventListener("click", () => selectTab(t));
  t.addEventListener("keydown", (e) => {
    let targetIdx = null;
    if (e.key === "ArrowRight") targetIdx = (idx + 1) % tabs.length;
    else if (e.key === "ArrowLeft") targetIdx = (idx - 1 + tabs.length) % tabs.length;
    else if (e.key === "Home") targetIdx = 0;
    else if (e.key === "End") targetIdx = tabs.length - 1;
    if (targetIdx !== null) {
      e.preventDefault();
      tabs[targetIdx].focus();
      selectTab(tabs[targetIdx]);
    }
  });
});

// Controls listeners
document.getElementById("workload-mode").addEventListener("change", (e) => {
  appState.mode = e.target.value;
  const resetKey = appState.mode === "linear" ? (appState.linearIdx = 3, LINEAR_KEYS[3]) : (appState.ringPos = 0, "v0");
  appState.stateKey = resetKey;
  appState.prevKey = resetKey;
  appState.prevBits = encodeAllForKey(resetKey, appState.mode, appState.linearIdx);
  for (const s of CODING_SPECS) {
    appState.lastMasks[s.id] = new Array(appState.prevBits[s.id].length).fill(0);
    appState.lastDeltas[s.id] = 0;
  }
  appState.pulseTimer = 0;
  updateUI({ half: 0, full: 0 });
});

document.getElementById("btn-next").addEventListener("click", () => stepDirection(1));
document.getElementById("btn-prev").addEventListener("click", () => stepDirection(-1));
document.getElementById("btn-center").addEventListener("click", () => {
  if (appState.mode === "linear") appState.linearIdx = 4;
  transitionTo("center");
});
document.getElementById("btn-sentinel").addEventListener("click", () => transitionTo("sentinel"));
document.getElementById("btn-reset-counts").addEventListener("click", () => {
  appState.totals = {
    steps: 0,
    halfSwings: 0,
    fullSwings: 0,
    flips: { hexgray3b: 0, canon6b: 0, offset6b: 0, onehot7b: 0, onehot9b: 0 },
    glitches: { hexgray3b: 0, canon6b: 0, offset6b: 0, onehot7b: 0, onehot9b: 0 },
  };
  updateUI();
});

const playBtn = document.getElementById("btn-play");
playBtn.addEventListener("click", () => {
  appState.autoPlay = !appState.autoPlay;
  playBtn.textContent = appState.autoPlay ? "Pause Auto-Step" : "Play Auto-Step";
});

const skewSlider = document.getElementById("skew-slider");
skewSlider.addEventListener("input", () => {
  appState.skewNorm = Number(skewSlider.value) / 100;
  document.getElementById("skew-readout").textContent = `${appState.skewNorm.toFixed(2)} cycles`;
});

// Raycasting for picking 3D nodes & H16 bars
const raycaster = new THREE.Raycaster();
const ptr = new THREE.Vector2();
renderer.domElement.addEventListener("pointerdown", (ev) => {
  const rect = renderer.domElement.getBoundingClientRect();
  ptr.x = ((ev.clientX - rect.left) / rect.width) * 2 - 1;
  ptr.y = -((ev.clientY - rect.top) / rect.height) * 2 + 1;
  raycaster.setFromCamera(ptr, camera);
  const hits = raycaster.intersectObjects(pickables.filter((o) => {
    let p = o;
    while (p) { if (!p.visible) return false; p = p.parent; }
    return true;
  }));
  if (hits.length > 0) {
    const ud = hits[0].object.userData;
    if (ud.targetKey) {
      if (RING_KEYS.includes(ud.targetKey)) appState.ringPos = RING_KEYS.indexOf(ud.targetKey);
      transitionTo(ud.targetKey);
    } else if (ud.h16Key) {
      appState.selectedH16 = ud.h16Key;
      renderH16Table();
    }
  }
});

// ResizeObserver for crisp canvas fitting
new ResizeObserver(() => {
  const w = stageEl.clientWidth;
  const h = Math.max(1, stageEl.clientHeight);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
  renderer.setSize(w, h);
}).observe(stageEl);

// Initialize readouts
updateUI();

// Animation loop
let lastTs = performance.now();
let linearDir = 1;
function tick(now) {
  requestAnimationFrame(tick);
  const dt = Math.min(0.05, (now - lastTs) / 1000);
  lastTs = now;

  if (appState.autoPlay) {
    appState.autoAccum += dt;
    if (appState.autoAccum >= 0.85) {
      appState.autoAccum = 0;
      if (appState.mode === "linear") {
        if (appState.linearIdx >= LINEAR_KEYS.length - 1) linearDir = -1;
        else if (appState.linearIdx <= 0) linearDir = 1;
        stepDirection(linearDir);
      } else {
        stepDirection(1);
      }
    }
  }

  appState.pulseTimer = Math.max(0, appState.pulseTimer - dt * 1.15);
  const st = HEX_STATES[appState.stateKey];

  // 1. Update Overview Arena rotors & wire bars
  for (const spec of CODING_SPECS) {
    const v = overviewViews[spec.id];
    const bits = appState.prevBits[spec.id];
    const mask = appState.lastMasks[spec.id];
    const flips = appState.lastDeltas[spec.id];

    const targetYaw = st.angle !== null ? st.angle : v.rotor.rotation.y;
    const isCentered = st.angle === null;
    v.rotor.visible = !isCentered;
    if (!isCentered) {
      if (appState.reducedMotion) {
        v.rotor.rotation.y = targetYaw;
      } else {
        const diff = Math.atan2(Math.sin(targetYaw - v.rotor.rotation.y), Math.cos(targetYaw - v.rotor.rotation.y));
        v.rotor.rotation.y += diff * Math.min(1, dt * 12);
      }
    }

    const hazardOpacity = flips >= 2 ? 0.25 + 0.65 * appState.pulseTimer : 0.0;
    v.hazardRing.material.opacity = hazardOpacity;

    for (let i = 0; i < v.wireBars.length; i++) {
      const bar = v.wireBars[i];
      const bitHigh = bits[i] === 1;
      const flipped = mask[i] === 1;
      const targetScaleY = bitHigh ? 1.75 : 0.65;
      bar.scale.y += (targetScaleY - bar.scale.y) * Math.min(1, dt * 14);
      bar.position.y = -0.92 + (flipped ? 0.16 * appState.pulseTimer : 0);
      if (flipped && appState.pulseTimer > 0.05) {
        bar.material.color.setHex(flips >= 2 ? 0xff4d6d : 0xfacc15);
        bar.material.emissive.setHex(flips >= 2 ? 0xff4d6d : 0xfacc15);
        bar.material.emissiveIntensity = 0.85 * appState.pulseTimer;
      } else {
        bar.material.color.setHex(bitHigh ? spec.color : 0x1e293b);
        bar.material.emissive.setHex(bitHigh ? spec.color : 0x000000);
        bar.material.emissiveIntensity = bitHigh ? 0.35 : 0.0;
      }
    }
  }

  // 2. Update Geometry Stage
  const cPos = CUBE_COORDS[st.code];
  cubeActiveBead.position.set(...cPos);
  if (st.angle !== null) {
    hexActiveBeacon.position.set(Math.cos(st.angle) * 1.7, -0.25, -Math.sin(st.angle) * 1.7);
    hexActiveBeacon.visible = true;
  } else if (appState.stateKey === "center") {
    hexActiveBeacon.position.set(0, -0.25, 0);
    hexActiveBeacon.visible = true;
  } else {
    hexActiveBeacon.visible = false;
  }

  // 3. Update Skew Lab pulses
  const progress = 1.0 - appState.pulseTimer; // 0 -> 1 after step
  for (const id of ["hexgray3b", "canon6b", "onehot9b"]) {
    const ln = skewLanes[id];
    const mask = appState.lastMasks[id];
    const flippedIndices = mask.map((v, idx) => (v ? idx : -1)).filter((i) => i >= 0);
    let inFlightCount = 0;

    for (let w = 0; w < ln.pulses.length; w++) {
      const p = ln.pulses[w];
      if (!mask[w] || appState.pulseTimer <= 0.01) {
        p.visible = false;
        continue;
      }
      p.visible = true;
      const order = flippedIndices.indexOf(w);
      const delayFrac = flippedIndices.length > 1 ? (order / (flippedIndices.length - 1)) * (appState.skewNorm * 0.48) : 0;
      const localT = Math.max(0, Math.min(1, (progress - delayFrac) / Math.max(0.25, 1 - delayFrac)));
      p.position.x = -4.6 + localT * 9.2;
      if (localT > 0 && localT < 1) inFlightCount += 1;
    }

    const tornNow = flippedIndices.length >= 2 && appState.skewNorm > 0.08 && appState.pulseTimer > 0.06;
    ln.rxBox.material.color.setHex(tornNow ? 0xff4d6d : 0x1e293b);
    ln.rxBox.material.emissive.setHex(tornNow ? 0xff4d6d : ln.lane.color);
    ln.rxBox.material.emissiveIntensity = tornNow ? 0.9 : 0.25;
  }

  // 4. Update Silicon Skyline highlight
  for (const cl of siliconClusters) {
    const selected = cl.key === appState.selectedH16;
    cl.plate.material.emissive.setHex(selected ? 0x38bdf8 : 0x000000);
    cl.plate.material.emissiveIntensity = selected ? 0.6 : 0;
  }

  controls.update();
  renderer.render(scene, camera);
}

requestAnimationFrame(tick);
