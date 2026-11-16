import json, sys

# Params arrive as JSON on stdin (and as BLOCK_<NAME> env vars).
params = json.load(sys.stdin)

# Print Markdown: it's shown in place of the block.
print(f"Hello, **{params['who']}**!")
print()
print("| Param | Value |")
print("|---|---|")
for key, value in params.items():
    print(f"| {key} | {value} |")
