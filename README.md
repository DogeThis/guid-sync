# What it does

Given two Unity projects, let's call them "main" (your actual project) and "subordinate" (the one you would like to import from), it

1. Crawls over both projects and scans through Unity YAML files and `.meta` files to find their UUIDs
2. Looking over matching file paths, we track that we need to do this mapping.
3. Then, we "sync" them, going through the subordinate project, updating the GUIDs in the `.meta` files and scans for usage of that GUID and replaces them.

This way we can export packages from the subordinate project that the main project can import without breaking references.

# Pitfalls
I assumed that all UUIDs are plain text in a predictable format. If the are any that don't match the regex, we will miss them.

# Usage

Usage: guid-sync <COMMAND>

Commands:
  scan    Scan projects and show GUID differences
  report  Generate detailed sync operations report
  sync    Synchronize GUIDs from main project to subordinate project
  help    Print this message or the help of the given subcommand(s)

`sync` is what actually drives the changes. `scan` and `report` are for development purposes.

Usage: guid-sync sync --main <MAIN> --subordinate <SUBORDINATE>, where MAIN and SUBORDINATE are paths to the Unity project folders. 
Unity project folders, for our purposes, contain an `Assets` folder.

--dry-run and --verbose are available as flags for this mode and they do what they say.