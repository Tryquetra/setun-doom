#!/usr/bin/env bash
# H14: alternative ternary representations for binary hardware.
# Adds 24-trit balanced adders in two extra codings (offset-coded, one-hot)
# and compares their synthesis cell counts with the canonical direct baseline
# of H7. Behavior is verified exhaustively at N=2 with yosys eval against a
# Python reference (243 cases per variant).
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD="fpga/build/h14"
mkdir -p "$BUILD"

count_cells() {
    local json=$1 top=$2
    python3 - "$json" "$top" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
cells = data["modules"][sys.argv[2]]["cells"].values()
counts = {}
for c in cells:
    counts[c["type"]] = counts.get(c["type"], 0) + 1
print(counts.get("LUT4", 0), counts.get("LUT1", 0),
      counts.get("IBUF", 0), counts.get("OBUF", 0))
PY
}

verify_offset() {
    python3 - <<'PY'
import itertools, re, subprocess, sys
N = 2
codes = (0, 1, 2)
trits = list(itertools.product(codes, repeat=N))

def vec_msb(trits):
    out = []
    for c in reversed(trits):          # most significant trit first
        out.append((c >> 1) & 1)       # code bit 1
        out.append(c & 1)              # code bit 0
    return "".join(map(str, out))

def ref(words, cin):
    y = []
    for i in range(N):
        s = words[0][i] + words[1][i] + cin - 3
        d = ((s + 1) % 3) - 1
        cin = (s - d) // 3 + 1
        y.append(d + 1)
    return vec_msb(y), cin

def run_eval(abits, bbits, cbits):
    r = subprocess.run(
        ["yosys", "-p",
         f"read_verilog fpga/gowin/trit_add_offset.v; "
         f"hierarchy -top trit_add_offset; chparam -set N 2; proc; opt; "
         f"eval -set a {N*2}'b{abits} -set b {N*2}'b{bbits} "
         f"-set cin 2'b{cbits}"],
        capture_output=True, text=True)
    if r.returncode != 0:
        print("yosys eval failed", r.stderr); sys.exit(2)
    got = {}
    for line in ((r.stdout or "") + (r.stderr or "")).splitlines():
        m = re.search(r"Eval result:\s+\\([^ ]+)\s*=\s*[0-9]+'([01]+)", line)
        if m:
            got[m.group(1)] = m.group(2)
    return got

for a, b, cin in itertools.product(trits, trits, codes):
    got = run_eval(vec_msb(a), vec_msb(b), format(cin, "02b"))
    ey, ec = ref((a, b), cin)
    ec = format(ec, "02b")
    if got.get("y") != ey or got.get("cout") != ec:
        print("MISMATCH", a, b, cin, got, (ey, ec)); sys.exit(3)
print("offset: 243 cases PASS")
PY
}

verify_onehot() {
    python3 - <<'PY'
import itertools, re, subprocess, sys
N = 2
oh = (0b001, 0b010, 0b100)
trits = list(itertools.product(oh, repeat=N))

def vec_msb(trits):
    out = []
    for c in reversed(trits):          # most significant trit first
        for k in (2, 1, 0):            # one-hot bit 2 down to 0
            out.append((c >> k) & 1)
    return "".join(map(str, out))

def val(c):
    return (1 if c == 0b010 else -1 if c == 0b100 else 0)

def enc(d):
    return 0b001 if d == 0 else 0b010 if d == 1 else 0b100

def ref(words, cin):
    y = []
    for i in range(N):
        s = val(words[0][i]) + val(words[1][i]) + val(cin)
        d = ((s + 1) % 3) - 1
        cin = enc((s - d) // 3)
        y.append(enc(d))
    return vec_msb(y), cin

def run_eval(abits, bbits, cbits):
    r = subprocess.run(
        ["yosys", "-p",
         f"read_verilog fpga/gowin/trit_add_onehot.v; "
         f"hierarchy -top trit_add_onehot; chparam -set N 2; proc; opt; "
         f"eval -set a {3*N}'b{abits} -set b {3*N}'b{bbits} "
         f"-set cin 3'b{cbits}"],
        capture_output=True, text=True)
    if r.returncode != 0:
        print("yosys eval failed", r.stderr); sys.exit(2)
    got = {}
    for line in ((r.stdout or "") + (r.stderr or "")).splitlines():
        m = re.search(r"Eval result:\s+\\([^ ]+)\s*=\s*[0-9]+'([01]+)", line)
        if m:
            got[m.group(1)] = m.group(2)
    return got

for a, b, cin in itertools.product(trits, trits, oh):
    got = run_eval(vec_msb(a), vec_msb(b), format(cin, "03b"))
    ey, ec = ref((a, b), cin)
    ec = format(ec, "03b")
    if got.get("y") != ey or got.get("cout") != ec:
        print("MISMATCH", a, b, cin, got, (ey, ec)); sys.exit(3)
print("onehot: 243 cases PASS")
PY
}

verify_offset
verify_onehot

printf '%-17s %-24s %6s %6s %6s %6s %6s\n' op code LUT4 LUT1 IBUF OBUF TOTAL
declare -A RESULT
for spec in \
    "add trit_add_direct fpga/gowin/trit_add.v" \
    "add trit_add_offset fpga/gowin/trit_add_offset.v" \
    "add trit_add_onehot fpga/gowin/trit_add_onehot.v"; do
    set -- $spec
    op=$1; top=$2; src=$3
    yosys -q -p "read_verilog $src; synth_gowin -top $top -json $BUILD/${top}.json" \
        > "$BUILD/${top}.yosys.log" 2>&1
    read -r lut4 lut1 ibuf obuf < <(count_cells "$BUILD/${top}.json" "$top")
    total=$((lut4 + lut1))
    RESULT[$top]="$lut4 $lut1 $ibuf $obuf $total"
    printf '%-17s %-24s %6s %6s %6s %6s %6s\n' \
        "$op" "$top" "$lut4" "$lut1" "$ibuf" "$obuf" "$total"
done

# METRIC lines consumed by the autoresearch harness
for top in trit_add_direct trit_add_offset trit_add_onehot; do
    read -r lut4 lut1 ibuf obuf total <<< "${RESULT[$top]}"
    echo "METRIC adder_lut_cells=$total"
    echo "METRIC adder_lut4=$lut4"
    echo "METRIC adder_lut1=$lut1"
    echo "METRIC adder_ibuf_obuf=$((ibuf + obuf))"
    echo "ASI variant=$top"
done
