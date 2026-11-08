- **Pasted HTML loses a document's `<title>` with its text.** The sanitiser dropped the tag but
  kept the text, so a full page pasted into a field began with its title; it now reads `<title>`
  as raw text to `</title>`, as a browser does, and drops it all.
