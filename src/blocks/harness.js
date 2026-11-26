// Loads a Node block script (CommonJS or ESM) as usual, then, if it exports run (or a default
// function), prints what it returns. Params come from BUFFER_BLOCK_INPUT, leaving stdin alone.
const { pathToFileURL } = require("node:url");
const path = require("node:path");

(async () => {
  const mod = await import(pathToFileURL(path.resolve(process.argv[1])).href);
  const run = [mod.run, mod.default, mod.default?.run].find((f) => typeof f === "function");
  if (!run) return;
  const result = await run(JSON.parse(process.env.BUFFER_BLOCK_INPUT || "{}"));
  if (result != null) process.stdout.write(String(result));
})().catch((e) => {
  console.error(e?.stack ?? String(e));
  process.exitCode = 1;
});
