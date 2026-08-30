# Browser extension — placeholder

Planned: a WebExtension for "continue this page on my other device", talking
to the daemon on localhost.

Nothing here yet. It is blocked on two decisions that should not be rushed:

1. **How the extension authenticates to the daemon.** A browser extension is
   not a trusted peer, and it must not be able to use the network protocol.
   The likely answer is a separate local token, not a device identity.
2. **The `open-url.v1` capability.** Handing a peer-supplied URL to
   `xdg-open` or an Android intent is a real attack surface. See T9 in the
   [threat model](../docs/security/THREAT_MODEL.md).
