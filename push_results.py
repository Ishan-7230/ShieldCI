#!/usr/bin/env python3
"""
push_results.py — Parse shield_results.json and POST to the ShieldCI frontend API.

Environment variables:
  SHIELDCI_API_URL  — Frontend base URL (e.g. http://localhost:3000)
  SHIELDCI_API_KEY  — API key for authentication
  SHIELDCI_REPO     — Repository full name (e.g. owner/repo)
  SHIELDCI_BRANCH   — Branch name
  SHIELDCI_COMMIT   — Commit SHA
  SHIELDCI_COMMIT_MSG — Commit message
  SHIELDCI_DURATION — Scan duration string
  SHIELDCI_TRIGGERED_BY — Who/what triggered this scan (default: PR)
"""

import json
import os
import sys
import urllib.request
import urllib.error

def main():
    results_file = os.environ.get("SHIELDCI_RESULTS_FILE", "shield_results.json")

    if not os.path.exists(results_file):
        print(f"ERROR: {results_file} not found. Did the scan run?")
        sys.exit(1)

    api_url = os.environ.get("SHIELDCI_API_URL", "").rstrip("/")
    api_key = os.environ.get("SHIELDCI_API_KEY", "")

    if not api_url:
        print("ERROR: SHIELDCI_API_URL not set")
        sys.exit(1)
    if not api_key:
        print("ERROR: SHIELDCI_API_KEY not set")
        sys.exit(1)

    with open(results_file, "r") as f:
        results = json.load(f)

    repo = os.environ.get("SHIELDCI_REPO", "unknown/repo")
    branch = os.environ.get("SHIELDCI_BRANCH", "main")
    commit = os.environ.get("SHIELDCI_COMMIT", "")
    commit_msg = os.environ.get("SHIELDCI_COMMIT_MSG", "")
    duration = os.environ.get("SHIELDCI_DURATION", "")
    triggered_by = os.environ.get("SHIELDCI_TRIGGERED_BY", "PR")

    payload = {
        "repo": repo,
        "branch": branch,
        "commit": commit[:7] if commit else "",
        "commitMessage": commit_msg,
        "status": results.get("status", "Clean"),
        "duration": duration,
        "triggeredBy": triggered_by,
        "reportMarkdown": results.get("report_markdown", ""),
        "vulnerabilities": results.get("vulnerabilities", []),
    }

    data = json.dumps(payload).encode("utf-8")
    endpoint = f"{api_url}/api/scans"

    req = urllib.request.Request(
        endpoint,
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(req) as response:
            body = response.read().decode("utf-8")
            print(f"SUCCESS: {response.status} — {body}")
    except urllib.error.HTTPError as e:
        body = e.read().decode("utf-8")
        print(f"ERROR: {e.code} — {body}")
        sys.exit(1)
    except urllib.error.URLError as e:
        print(f"ERROR: Could not connect to {endpoint} — {e.reason}")
        sys.exit(1)

if __name__ == "__main__":
    main()
