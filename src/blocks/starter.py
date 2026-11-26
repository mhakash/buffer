import json, sys

# Params arrive as JSON on stdin (and as BLOCK_<NAME> env vars).
params = json.load(sys.stdin)

# An ```input fence in the output becomes controls; a change re-runs the block with it.
print("```input")
print(f"text who label=Greet default={json.dumps(params['who'], ensure_ascii=False)}")
print("```")
print()

# Print Markdown: it's shown in place of the block. (Or define run(params) and return it.)
print(f"Hello, **{params['who']}**!")
print()
print("| Param | Value |")
print("|---|---|")
for key, value in params.items():
    print(f"| {key} | {value} |")
