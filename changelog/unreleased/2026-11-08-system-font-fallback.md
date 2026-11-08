- **Text draws characters no loaded font has in the font the system picks for them.** A
  character every loaded font lacks (Ethiopic, Cherokee, Yi, Myanmar, a rare CJK ideograph) asks
  fontconfig, Core Text or DirectWrite once, and that character, alone, shapes in the font it
  names from then on. Latin text never asks.
