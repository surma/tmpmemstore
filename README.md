# tmpmemstore

An in-memory data store that exposes sensitive data to subprocesses via UNIX domain sockets, ensuring data never touches the disk.

## Overview

tmpmemstore is a Rust utility designed to ephemerally store data like passphrases that need to be accessed by subprocesses without being written to disk or exposed in process arguments.

## Features

- **Secure Input**: Uses hidden password input to prevent shoulder surfing
- **Memory-Only Storage**: Data is kept in memory and never written to disk
- **Permission Protection**: UNIX domain sockets are restricted to owner-only access (0600)
- **Process Ancestry Verification**: Only child processes can access the stored data (macOS/Linux)

## Usage

### Basic Example

```bash
# Prompts for password input (hidden from terminal)
tmpmemstore run -- ./decrypt-files.sh

# Inside decrypt-files.sh, retrieve the stored data:
passphrase=$(tmpmemstore retrieve)
```

### Real-World Example

```bash
# Use with GPG to decrypt multiple files without re-entering passphrase
tmpmemstore run -- bash -c '
  for file in *.gpg; do
    gpg --batch --yes --passphrase-fd 0 -d "$file" > "${file%.gpg}" < <(tmpmemstore retrieve)
  done
'
```

---

Apache 2.0
