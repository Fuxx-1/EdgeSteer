# EdgeSteer interface

The EdgeSteer configuration interface, built with Flutter.

This process owns the window only. The DNS engine, the system DNS
registration, and the configuration file belong to the resident Rust
`edgesteer-agent`, which keeps running after this window closes. Every command
goes through the agent's authenticated loopback control protocol
(`lib/agent_client.dart` mirrors `src/control.rs`), so the interface never
touches the resolver or the configuration file directly and always validates
against the exact parser the DNS service uses.

```sh
flutter pub get
flutter analyze
flutter test
flutter build macos --release
```

`flutter build macos --release` writes `build/macos/Build/Products/Release/EdgeSteer.app`.
`packaging/macos/build-app.sh` embeds that bundle as a nested application inside
the outer `EdgeSteer.app`, next to `edgesteer-agent` and `edgesteer-engine`.

`EDGESTEER_UI_PAGE=<overview|resolvers|ruleSets|cloudflare|json|system>` opens a
specific page on launch. It exists for screenshot-based verification; normal
launches always start on the overview.
