# Plugin Review Checklist

Use this checklist when reviewing community plugin submissions.

## Manifest

- [ ] Unique `id` (lowercase, hyphen-separated)
- [ ] Valid semver `version` and `api_version: 1`
- [ ] `entry` binary exists and is executable
- [ ] Only requests known v1 capabilities
- [ ] Description explains user value clearly

## Security

- [ ] No shell invocation of untrusted user input
- [ ] No network access unless explicitly documented and justified
- [ ] No direct PipeWire/pactl calls (use host capabilities only)
- [ ] Fails gracefully without blocking core routing

## Capabilities

- [ ] Each requested capability is justified in the PR/README
- [ ] Minimal capability set (not "request everything")
- [ ] High-impact capabilities (`routing.apply`, `profile.write`) not used in v1

## Runtime

- [ ] Responds to `initialize` and `shutdown` within 5 seconds
- [ ] Handles malformed RPC without crashing
- [ ] Does not write outside plugin config dir or audit log

## UX

- [ ] UI panels use plain language; no hidden automation
- [ ] Errors surfaced to user via plugin status in Settings

## Documentation

- [ ] README with install steps and capability justification
- [ ] Tested with `PIPE_DECK_USE_MOCK=1`
