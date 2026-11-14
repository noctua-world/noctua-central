# Undo

Undo takes back your last step. A step is one thing you asked Noctua to do,
with everything it changed at once: the values it wrote, and the files it
made.

## What it takes back

- every value the step changed, put back as it was before;
- the files the step made, which are removed once no other step holds them.

## When it stops

Undo stops, and changes nothing, when:

- the step reached outside this device, such as a message sent or a request
  made to another service, and nothing was registered to take that back;
- something changed one of the step's values after it, so putting it back
  would undo that later change too;
- you may no longer do one of the things the step did. Undo asks again, as you
  are now, before it takes anything back.

## Redo

Redo puts back the step you took back, as long as you have done nothing new
since.
