# Runs a Python block script as usual, then, if it defines run(params), prints what run returns.
# Params come from BUFFER_BLOCK_INPUT, leaving stdin for scripts that read it themselves.
import inspect, json, os, runpy, sys

path = sys.argv[1]
sys.argv = sys.argv[1:]
sys.path.insert(0, os.path.dirname(os.path.abspath(path)))
run = runpy.run_path(path, run_name="__main__").get("run")
if callable(run):
    params = json.loads(os.environ.get("BUFFER_BLOCK_INPUT") or "{}")
    result = run(params) if inspect.signature(run).parameters else run()
    if inspect.isawaitable(result):
        import asyncio

        async def wait():
            return await result

        result = asyncio.run(wait())
    if result is not None:
        sys.stdout.write(str(result))
