# Concepts

> Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## Waiting

### Resource
A thing wait-on waits on, such as a file, an http(s) endpoint, a tcp port, a unix socket, or a command, named by a prefixed string that selects how it is checked.

A run succeeds only when every resource reports ready; any one resource failing to get there before the overall timeout fails the whole run.

### Poll
One scheduled check of a resource; polls repeat on a fixed interval until the resource is ready or the run ends.

For file, http(s), tcp, and socket resources, a new poll does not cancel a check still in flight from an earlier poll, so a resource slower than the interval can still succeed; when concurrent in-flight checks are capped, extra polls wait their turn rather than being dropped. Command resources are the exception: while a command runs, later polls are dropped, so a command never runs concurrently with itself.

### Reverse mode
Waiting for resources to become unavailable instead of available, by inverting the result of every check.

Because a failed check counts as success in reverse mode, any error in a check, including one caused by the check itself being cancelled or timing out, reads as "unavailable". Tests of reverse mode therefore assert on what the server saw, not only on the outcome.
