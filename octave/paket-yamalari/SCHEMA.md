# Package patch manifests

`scripts/patch_packages.py` operates on a private, real `.packages` directory.
It rejects symlinked package roots, targets, intermediate directories, sources,
and registry files. The Kernel does not apply patches.

Schema 1 remains the format for editing existing package files. Schema 2 adds
an optional `creates` list alongside `files` and `patches`:

```json
{
  "schema": 2,
  "package": "example-1.0.0",
  "files": [],
  "patches": [],
  "creates": [
    {
      "path": "@example/example.m",
      "source": "files/@example/example.m",
      "sha256": "<64 lowercase hexadecimal characters: SHA-256 of source bytes>"
    }
  ]
}
```

This is a format example, not an applicable manifest. `source` is relative to
the versioned patch-set directory; `path` is relative to the installed package.
All source bytes must match their declared hashes. Target paths must be unique,
non-overlapping, and outside `.indymat-patches`. Creates-only sets are allowed.
Existing-file entries retain `pristine_sha256` and `patched_sha256`; their
exact-context unified diffs remain listed in `patches`.

`--check` never writes. Creation states are:

- `missing`: no target file; apply creates it and any missing parent directories.
- `created`: exact registered bytes owned by this patch mechanism.
- `present`: exact bytes already exist without ownership; apply leaves them
  unowned, and revert preserves them.
- `outdated`: verified owned bytes from the previously registered patch set;
  apply upgrades them to the new source and ownership hash.
- `modified`: unexpected bytes; CLI check exits 1 and apply/revert refuse the
  entire transaction. No target, backup, or registry changes are committed.

The registry records the manifest. `.indymat-patches/created.json` records only
paths actually created by this mechanism and their hashes; ledger entries must
agree with the registered manifest. Hash equality alone never grants ownership.
A missing/corrupt ownership record does not authorize deletion of existing files.

`--revert` restores verified existing-file backups and removes only verified
owned creation targets. It preserves preexisting identical files, unrelated
files, directories, and audit metadata. Repeated apply/check/revert is safe.
Upgrades may change an owned creation's bytes, but must retain its path in the
new manifest; dropping owned creation paths is refused. Revert the old set first
before intentionally removing a creation from a later set.

All inputs and saved state are validated before writes and checked again before
commit. New-file writes, edits, deletion, ownership, and registry changes share
the transaction rollback. Atomic replacements and rollback preserve existing
file modes; newly created files use mode 0600. Apply patches while the private
package tree is not being edited concurrently. `relocate_packages.py` and the
package registry's architecture prefixes are independent of this mechanism.
