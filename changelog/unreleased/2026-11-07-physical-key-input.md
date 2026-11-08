- **`Runtime::physical_key_input` is public.** A test or embedder can deliver a key with its
  physical code, logical key and `Tx::key_base`, as a native host does, so a handler that reads
  `Tx::physical_key` is testable headlessly.
