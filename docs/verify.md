# Verify your download

Every release carries `pith.exe`, a portable zip, the Linux tarball and one `SHA256SUMS` that lists all three. GitHub Actions also signs a build provenance attestation for each of these files, `SHA256SUMS` included. It says which commit and which workflow of this repository built the file.

Check where the file came from (needs the [GitHub CLI](https://cli.github.com)):

```
gh attestation verify pith.exe --repo kunalgoesbyken/pith --signer-workflow kunalgoesbyken/pith/.github/workflows/release.yml
```

Use the same command with another file's name to check that file, including `SHA256SUMS`.

Check the file was not damaged or swapped in transit, with tools Windows already has:

```
certutil -hashfile pith.exe SHA256
certutil -hashfile pith-0.1.0-x64-portable.zip SHA256
```

Each hash must equal that file's own line in `SHA256SUMS` from the same release (use your version in the zip's name). The checksum alone proves less than the attestation: `SHA256SUMS` sits on the same release page, so whoever could replace the exe could replace the list too. Check the list's own attestation too.

**About SmartScreen and Smart App Control.** `pith.exe` is not code-signed. Signing certificates cost money every year, and Pith is a free project. SmartScreen judges an unsigned program by how many people have already run that exact file, so a new release shows "Windows protected your PC", and it can show again on the next release because each build is a new file. The warning means "not known yet", not "known bad". After the attestation or the hash checks out, choose More info, then Run anyway. Extracting the zip with Windows Explorer copies the download's mark of the web onto `pith.exe`, so the same prompt appears for the extracted file.

Windows 11 with Smart App Control turned on is stricter: it blocks unsigned programs and offers no Run anyway. Then your choices are to build Pith from source (see [Build and test](architecture.md#build-and-test)), or to turn Smart App Control off, which Windows only lets you undo by resetting the PC. We have not verified how Smart App Control treats a binary you built yourself.
