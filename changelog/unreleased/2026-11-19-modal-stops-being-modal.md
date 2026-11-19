- A modal that stays mounted but drops `El::modal` (a panel that slides away inert) now returns focus to what had it before it opened, as removing it does.
  It kept its place on the modal stack, so focus inside it was dropped, and opening it again saved no new place to return to.
