# Canonical TOML form saves

AI Scheduler form saves will rewrite `config.toml` as canonical TOML rather than preserving comments and hand formatting. The raw TOML editor preserves text while editing and validates before save, but structured form edits own the config shape and may normalize formatting. This keeps v1 implementation focused on reliable config changes instead of comment-preserving TOML patching.
