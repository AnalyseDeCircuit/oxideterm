{
  lib,
  rustPlatform,
  stdenv,
  alsa-lib,
  cacert,
  clang,
  cmake,
  copyDesktopItems,
  dbus,
  fontconfig,
  freetype,
  gst_all_1,
  krb5,
  libclang,
  libGL,
  libunwind,
  libx11,
  libxcb,
  libxcursor,
  libxfixes,
  libxinerama,
  libxkbcommon,
  libxrandr,
  makeDesktopItem,
  makeWrapper,
  nasm,
  openh264,
  openssl,
  perl,
  udev,
  pkg-config,
  vulkan-loader,
  wayland,
}:

assert lib.assertMsg stdenv.hostPlatform.isLinux
  "OxideTerm's Nix package is supported only on Linux";

let
  target = stdenv.hostPlatform.rust.rustcTarget;
  gstPluginsBase = gst_all_1.gst-plugins-base;
  gstPluginsGood = gst_all_1.gst-plugins-good;
  gstLibav = gst_all_1.gst-libav;
  gstreamer = gst_all_1.gstreamer;
  gstPlugins = [
    gstreamer
    gstPluginsBase
    gstPluginsGood
    gstLibav
  ];
  runtimeLibs = [
    alsa-lib
    dbus
    fontconfig
    freetype
    gstPluginsBase
    gstPluginsGood
    gstLibav
    gstreamer
    krb5
    libGL
    libunwind
    libx11
    libxcb
    libxcursor
    libxfixes
    libxinerama
    libxkbcommon
    libxrandr
    openh264
    openssl
    udev
    vulkan-loader
    wayland
  ];
in
rustPlatform.buildRustPackage {
  pname = "oxideterm";
  version = (builtins.fromTOML (builtins.readFile ../Cargo.toml)).workspace.package.version;
  src = ../.;

  cargoLock = {
    lockFile = ../Cargo.lock;
    # rustPlatform.buildRustPackage builds in a network-isolated sandbox.
    # While crates.io dependencies have checksums recorded in Cargo.lock, Git
    # dependencies require fixed-output derivation hashes defined in outputHashes.
    #
    # When updating or adding Git dependencies in Cargo.toml / Cargo.lock:
    # 1. Update Cargo.toml and Cargo.lock first.
    # 2. Run `nix build .#oxideterm -L --show-trace`.
    # 3. If a hash mismatch occurs, check the locked Git revision and update
    #    the matching entry below with the `got:` sha256 output.
    # 4. Run `nix flake check -L` to ensure package checks pass.
    outputHashes = {
      "russh-0.63.0" = "sha256-oMUSzDpWWh9/W+HEipJrU2A8CRbpyCoJVeDchIbBsNM=";
    };
  };

  nativeBuildInputs = [
    clang
    cmake
    copyDesktopItems
    libclang
    makeWrapper
    nasm
    perl
    pkg-config
  ];

  buildInputs = runtimeLibs;

  cargoBuildFlags = [
    "-p"
    "oxideterm-gpui-app"
    "-p"
    "oxideterm-cli"
    "--bins"
  ];

  cargoTestFlags = [
    "-p"
    "oxideterm-update"
  ];

  strictDeps = true;
  LIBCLANG_PATH = "${libclang.lib}/lib";

  postInstall = ''
    resource_root="$out/bin/resources"
    target_triple="${target}"

    install -d "$resource_root/agents"
    install -d "$resource_root/icons"
    install -d "$resource_root/cli-bin/$target_triple"

    cp -R crates/oxideterm-gpui-app/resources/agents/. "$resource_root/agents/"
    cp -R crates/oxideterm-gpui-app/resources/icons/. "$resource_root/icons/"

    printf 'nix\n' > "$out/bin/PACKAGE_KIND"
    install -Dm644 LICENSE "$out/share/licenses/oxideterm/LICENSE"
    install -Dm644 NOTICE "$out/share/licenses/oxideterm/NOTICE"
    install -Dm644 THIRD_PARTY_NOTICES.md "$out/share/licenses/oxideterm/THIRD_PARTY_NOTICES.md"
    install -Dm644 README.md "$out/share/doc/oxideterm/README.md"

    install -Dm644 crates/oxideterm-gpui-app/resources/icons/32x32.png \
      "$out/share/icons/hicolor/32x32/apps/oxideterm.png"
    install -Dm644 crates/oxideterm-gpui-app/resources/icons/64x64.png \
      "$out/share/icons/hicolor/64x64/apps/oxideterm.png"
    install -Dm644 crates/oxideterm-gpui-app/resources/icons/128x128.png \
      "$out/share/icons/hicolor/128x128/apps/oxideterm.png"
    install -Dm644 crates/oxideterm-gpui-app/resources/icons/128x128@2x.png \
      "$out/share/icons/hicolor/256x256/apps/oxideterm.png"

    wrapProgram "$out/bin/oxideterm" \
      --set SSL_CERT_FILE "${cacert}/etc/ssl/certs/ca-bundle.crt" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}"

    wrapProgram "$out/bin/oxideterm-native" \
      --set OXIDETERM_CLI_BIN "$out/bin/oxideterm" \
      --set SSL_CERT_FILE "${cacert}/etc/ssl/certs/ca-bundle.crt" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}" \
      --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${lib.makeSearchPathOutput "lib" "lib/gstreamer-1.0" gstPlugins}"

    ln -s "$out/bin/oxideterm" "$resource_root/cli-bin/$target_triple/oxideterm"
  '';

  desktopItems = [
    (makeDesktopItem {
      name = "oxideterm";
      exec = "oxideterm-native %U";
      icon = "oxideterm";
      desktopName = "OxideTerm";
      comment = "AI-native workspace for local shells and remote machines";
      categories = [
        "Development"
        "TerminalEmulator"
        "Network"
      ];
      mimeTypes = [
        "x-scheme-handler/ssh"
        "x-scheme-handler/telnet"
        "x-scheme-handler/mosh"
        "x-scheme-handler/rdp"
        "x-scheme-handler/vnc"
      ];
      startupWMClass = "OxideTerm";
    })
  ];

  meta = {
    description = "AI-native workspace for local shells and remote machines";
    homepage = "https://github.com/AnalyseDeCircuit/oxideterm";
    license = lib.licenses.gpl3Only;
    mainProgram = "oxideterm-native";
    platforms = lib.platforms.linux;
  };
}
