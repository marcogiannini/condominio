#!/usr/bin/env bash
# Crea Gestione_Condominio-x86_64.AppImage. Eseguire dalla radice del progetto.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release --locked
APPDIR=target/AppDir
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
cp target/release/condominio "$APPDIR/usr/bin/condominio"
cp packaging/condominio.desktop "$APPDIR/condominio.desktop"
cp packaging/condominio.png "$APPDIR/condominio.png"
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/condominio" "$@"
EOF
chmod +x "$APPDIR/AppRun"

TOOL=target/appimagetool
if [ ! -x "$TOOL" ]; then
  curl -L -o "$TOOL" https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
  chmod +x "$TOOL"
fi
# --appimage-extract-and-run: funziona anche senza FUSE (CI, container)
ARCH=x86_64 "$TOOL" --appimage-extract-and-run "$APPDIR" Gestione_Condominio-x86_64.AppImage
echo "Creato: Gestione_Condominio-x86_64.AppImage"
