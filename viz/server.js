import path from "node:path";

const ROOT = import.meta.dir;
const PORT = Number(process.env.PORT || 4176);

Bun.serve({
  port: PORT,
  fetch(req) {
    const pathname = new URL(req.url).pathname;
    const rel = pathname === "/" ? "index.html" : pathname.replace(/^\/+/, "");
    const resolved = path.resolve(ROOT, rel);
    if (!resolved.startsWith(ROOT)) {
      return new Response("Forbidden", { status: 403 });
    }
    return new Response(Bun.file(resolved));
  },
});

console.log(`Serving setun-doom HexGray3 vs Codings 3D Visualizer at http://localhost:${PORT}`);
