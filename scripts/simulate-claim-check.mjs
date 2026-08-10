#!/usr/bin/env node
// Simulates .github/workflows/claim-check.yml locally against the real GitHub API,
// so the check's logic can be exercised without opening a PR or switching accounts.
// Mirrors that workflow's github-script step; keep the two in sync if either changes.
//
// Auth comes from the local `gh` CLI's own stored token — this script never reads,
// stores, or prints one itself.
//
// Usage:
//   node scripts/simulate-claim-check.mjs --pr 454
//   node scripts/simulate-claim-check.mjs --title "[noissue] - fix typo" --author someuser --body "Closes #1"
//   node scripts/simulate-claim-check.mjs --title "[#450] - add tests" --author vjymisal0 --body "Closes #451"

import { execFileSync } from "node:child_process";

const OWNER = "LunarVagabond";
const REPO = "Pipe-Deck";

function parseArgs(argv) {
  const out = {};
  for (let i = 0; i < argv.length; i += 2) {
    out[argv[i].replace(/^--/, "")] = argv[i + 1];
  }
  return out;
}

// execFileSync with an argv array (no shell) — nothing here is string-interpolated
// into a command line, so PR titles/bodies/usernames can't reach a shell even if
// they contained metacharacters.
function ghApi(path) {
  const raw = execFileSync("gh", ["api", path], { encoding: "utf8" });
  return JSON.parse(raw);
}

function main() {
  const args = parseArgs(process.argv.slice(2));

  let title, body, author;

  if (args.pr) {
    if (!/^\d+$/.test(args.pr)) {
      console.error("--pr must be a plain issue/PR number.");
      process.exit(2);
    }
    const pr = ghApi(`repos/${OWNER}/${REPO}/pulls/${args.pr}`);
    title = pr.title || "";
    body = pr.body || "";
    author = pr.user.login;
    console.log(`Loaded PR #${args.pr}: "${title}" by @${author}`);
  } else {
    title = args.title || "";
    body = args.body || "";
    author = args.author;
    if (!author) {
      console.error(
        "Need --author (or --pr) to simulate a permission/assignee check.",
      );
      process.exit(2);
    }
    console.log(`Simulated PR: "${title}" by @${author}`);
  }
  console.log("---");

  // --- mirror of claim-check.yml's script, verbatim logic ---

  if (/^(\[noissue\]|\[hotfix\]) - .+/.test(title)) {
    let perm;
    try {
      perm = ghApi(
        `repos/${OWNER}/${REPO}/collaborators/${encodeURIComponent(author)}/permission`,
      );
    } catch (error) {
      console.log(
        `FAIL: could not look up @${author}'s permission (${error.message})`,
      );
      process.exit(1);
    }
    if (perm.permission === "admin" || perm.permission === "write") {
      console.log(`PASS: exempt — @${author} has "${perm.permission}" access.`);
      process.exit(0);
    }
    console.log(
      `FAIL: [noissue]/[hotfix] titles are restricted to the maintainer and named core devs. ` +
        `@${author} has "${perm.permission}" access (needs admin/write) — this title isn't allowed.`,
    );
    process.exit(1);
  }

  const keywordPattern =
    /\b(close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s+#(\d+)/gi;
  const issueNumbers = new Set();
  let match;
  while ((match = keywordPattern.exec(body)) !== null) {
    issueNumbers.add(parseInt(match[2], 10));
  }

  if (issueNumbers.size === 0) {
    console.log(
      "FAIL: no issue reference found (need e.g. 'Closes #123' in the PR body).",
    );
    process.exit(1);
  }

  const problems = [];
  for (const issue_number of issueNumbers) {
    let issue;
    try {
      issue = ghApi(`repos/${OWNER}/${REPO}/issues/${issue_number}`);
    } catch (error) {
      problems.push(
        `#${issue_number} could not be looked up (${error.message})`,
      );
      continue;
    }
    const assignees = issue.assignees.map((a) => a.login);
    if (!assignees.includes(author)) {
      const assigneeList = assignees.length
        ? assignees.map((l) => `@${l}`).join(", ")
        : "nobody";
      problems.push(
        `#${issue_number} is assigned to ${assigneeList}, not @${author}`,
      );
    }
  }

  if (problems.length > 0) {
    console.log("FAIL: this PR references an issue not claimed by the author:");
    for (const p of problems) console.log(`  - ${p}`);
    process.exit(1);
  }

  console.log(
    `PASS: @${author} is assigned to every referenced issue (${[...issueNumbers].join(", ")}).`,
  );
  process.exit(0);
}

main();
