# Routine delete vs config removal

Deleting a routine through the app UI will delete that routine and its stored run history after confirmation. Removing a routine from the raw TOML config will not silently purge run history; raw config editing changes routine definitions, not historical data. This prevents a manual config mistake from destroying audit history while still allowing the explicit UI delete command to be truly destructive.
