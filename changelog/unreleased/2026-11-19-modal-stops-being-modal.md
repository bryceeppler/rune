- A modal that stays mounted but drops `El::modal` now closes on the modal stack as removing it does: focus left on one of its controls that stays usable stays there, and otherwise returns to what had it before the modal opened.
  It kept its place on the stack, so focus dropped inside it as it went inert was never returned, and opening it again saved no new place to return to.
