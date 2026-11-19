- A closing modal returns focus to what had it before only while that control is still usable; when it has turned inert, focus clears and the next Tab finds the first usable control.
  It returned focus to an inert field, whether the modal left the tree or stayed mounted and dropped `El::modal`, and typing then wrote into that field.
