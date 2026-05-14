# Pure Rust Rewrite

The existing Python codebase (bacpypes + monkey-patched broadcast forwarding) produces malformed BACnet packets that Wireshark cannot decode, and the threading model makes debugging opaque. We are rewriting the project in pure Rust using the rusty-bacnet library.

**Why Rust over Python + rusty-bacnet bindings**: rusty-bacnet's Python API is 2 months old and unproven in production. A pure Rust binary gives us a single statically-linked executable for deployment, compile-time correctness guarantees, and direct access to rusty-bacnet's full API surface — including its BTL compliance harness with 5,500+ tests.

**Why Rust over C + bacnet-stack**: bacnet-stack has incomplete BACnet/SC support, while rusty-bacnet implements the full SC spec (hub, spoke, TLS 1.3). Rust's ownership model eliminates the memory safety class of bugs that would be concerning in a network-facing C application handling untrusted BACnet traffic.
