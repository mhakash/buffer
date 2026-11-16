"""Jira tickets matching a JQL query, as a Markdown table.

Env: JIRA_URL (base URL), JIRA_TOKEN (personal access token; keychain:<service> keeps it in the
keychain), JIRA_EMAIL (Jira Cloud only: sends email + API token instead of a bearer token).
Params (stdin JSON): jql, limit.
"""
import base64
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

params = json.load(sys.stdin)
base = os.environ.get("JIRA_URL", "").rstrip("/")
token = os.environ.get("JIRA_TOKEN", "")
email = os.environ.get("JIRA_EMAIL", "")
if not base or not token:
    sys.exit("Set JIRA_URL and JIRA_TOKEN in this block's Environment.")

query = {
    "jql": params.get("jql") or "assignee = currentUser() ORDER BY updated DESC",
    "maxResults": int(params.get("limit") or 10),
    "fields": "summary,status,assignee,updated",
}
# Jira Cloud retired /rest/api/2/search; Server / Data Center still use it.
path = "/rest/api/3/search/jql" if base.endswith(".atlassian.net") else "/rest/api/2/search"
request = urllib.request.Request(f"{base}{path}?{urllib.parse.urlencode(query)}")
if email:
    basic = base64.b64encode(f"{email}:{token}".encode()).decode()
    request.add_header("Authorization", f"Basic {basic}")
else:
    request.add_header("Authorization", f"Bearer {token}")
request.add_header("Accept", "application/json")

try:
    with urllib.request.urlopen(request, timeout=20) as response:
        issues = json.load(response).get("issues", [])
except urllib.error.HTTPError as err:
    body = err.read().decode(errors="replace")[:500]
    sys.exit(f"Jira answered {err.code} {err.reason}\n{body}")
except urllib.error.URLError as err:
    sys.exit(f"Couldn't reach {base}: {err.reason}")


def cell(text):
    return str(text or "").replace("|", "\\|").replace("\n", " ")


if not issues:
    print("_No matching tickets._")
    sys.exit()

print("| Key | Summary | Status | Assignee | Updated |")
print("|---|---|---|---|---|")
for issue in issues:
    fields = issue.get("fields", {})
    key = issue.get("key", "")
    status = (fields.get("status") or {}).get("name", "")
    assignee = (fields.get("assignee") or {}).get("displayName", "—")
    updated = (fields.get("updated") or "")[:10]
    print(
        f"| [{key}]({base}/browse/{key}) | {cell(fields.get('summary'))} "
        f"| {cell(status)} | {cell(assignee)} | {updated} |"
    )
