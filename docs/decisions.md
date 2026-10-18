# Decisions

Why vedi is the way it is, and what it chose not to be.

## Inline images

Shelved 2026-10-18 after a working prototype of the kitty graphics protocol.

Reading illustrated Markdown is the one use, too narrow for the cost in a pager
for viewing, selecting and copying text. The prototype touched navigation,
layout, resizing, startup negotiation and one-page detection, and every later
change there would carry image cases. The protocol's file paths and raw payloads
widen the attack surface.
