# Verify activity history

Noctua records every action it takes for you in an activity history. Each
entry is linked to the one before it, so an entry that is changed, removed or
put out of order breaks the link after it.

Verifying the history reads it from its start, or from an entry you name, and
checks every link. It changes nothing.

## What it answers

- how many entries it checked, when every link holds;
- otherwise, the first entry whose link does not hold, and why.

## When a link does not hold

A broken link means the history was changed after it was written. Noctua does
not repair it, because nothing can tell which version is the true one. Tell
whoever set up your Noctua.
