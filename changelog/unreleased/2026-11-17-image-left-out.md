- **`ClipboardContent::image_left_out` says a paste left out an image too large to carry**, over `MAX_IMAGE_BYTES` or the pixel limit, so an app can explain why nothing pasted instead of doing nothing.
  Such a paste now reaches the paste hooks even when the image was all the clipboard held.
