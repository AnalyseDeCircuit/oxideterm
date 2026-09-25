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
  openssl,
  perl,
  udev,
  pkg-config,
  vulkan-loader,
  wayland,
  source ? ../.,
}:

assert lib.assertMsg stdenv.hostPlatform.isLinux
  "OxideTerm's Nix package is supported only on Linux";

let
  target = stdenv.hostPlatform.rust.rustcTarget;
  gstPluginsBase = gst_all_1.gst-plugins-base;
  gstreamer = gst_all_1.gstreamer;
  runtimeLibs = [
    alsa-lib
    dbus
    fontconfig
    freetype
    gstPluginsBase
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
    openssl
    udev
    vulkan-loader
    wayland
  ];
in
rustPlatform.buildRustPackage {
  pname = "oxideterm";
  version = (builtins.fromTOML (builtins.readFile (source + "/Cargo.toml"))).workspace.package.version;
  src = source;

  cargoLock = {
    lockFile = ../Cargo.lock;
    outputHashes = {
      "ironrdp-0.17.0" = "sha256-gBkwaq6m1iunsQ2Xz/5l5ajuI3uRmYkdlFdiE5wx7to=";
      "russh-0.63.0" = "sha256-o9p0ocNLF0QigbvykApRF+OyVbJ9aFgybaf8I+f17Do=";
      "wasm_thread-0.3.3" = "sha256-+lRLCIk0S6Y5ORYjDKsYYHia2FtoSoh+rWkQh7mnPBE=";
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
    "-p"
    "oxideterm-rdp-helper"
    "-p"
    "oxideterm-vnc-helper"
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
    install -d "$resource_root/helpers/$target_triple"

    cp -R crates/oxideterm-gpui-app/resources/agents/. "$resource_root/agents/"
    cp -R crates/oxideterm-gpui-app/resources/icons/. "$resource_root/icons/"

    install -Dm755 "target/$target_triple/release/oxideterm" \
      "$resource_root/cli-bin/$target_triple/oxideterm"
    install -Dm755 "target/$target_triple/release/oxideterm-rdp-helper" \
      "$resource_root/helpers/$target_triple/oxideterm-rdp-helper"
    install -Dm755 "target/$target_triple/release/oxideterm-vnc-helper" \
      "$resource_root/helpers/$target_triple/oxideterm-vnc-helper"

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

    wrapProgram "$out/bin/oxideterm-native" \
      --set OXIDETERM_CLI_BIN "$out/bin/oxideterm" \
      --set SSL_CERT_FILE "${cacert}/etc/ssl/certs/ca-bundle.crt" \
      --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}" \
      --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : "${gstPluginsBase}/lib/gstreamer-1.0"
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
