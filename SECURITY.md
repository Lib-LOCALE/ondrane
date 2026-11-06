# Security policy

Ondrane is preview software. Security fixes target the latest preview and the main branch.

Report suspected vulnerabilities privately through GitHub’s **Security → Report a vulnerability** for this repository. Include a minimal reproduction, affected versions and the expected impact. Do not post credentials or private media in an issue.

Local files and network streams are decoded by the installed libVLC and its plugins; keep VLC and system multimedia packages updated. The optional web remote uses plain HTTP on the local network. Enable it only on a trusted network, with a password, and do not expose it to the Internet. Screen/window capture uses desktop permission portals.
