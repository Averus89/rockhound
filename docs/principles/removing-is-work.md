# Removing is part of the work

What a change makes obsolete goes in the same PR. What cannot go yet gets a Cleanup issue with `Ready when` and `How to verify`. Not sure whether something is still used? Measure it first, then remove it. A flag is deleted in the PR that turns its change on for everyone. This holds for notes, rules, checks and documents as much as for code.

Why: agents add readily and rarely delete; everything left behind must be read, tested and understood by whoever comes next.
