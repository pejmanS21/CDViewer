#!/usr/bin/env bash
# Stage a portable DICOM-viewer CD/DVD payload at dist/cd-staging/.
#
# Usage:
#   dist/make-cd.sh                    # uses the host-platform release build
#   dist/make-cd.sh path/to/dicom/dir  # also copies that folder in as DICOM/
#
# The script never burns anything — it just produces a directory you can
# point your burner (or `genisoimage` / `mkisofs` / `hdiutil`) at.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STAGE="$REPO_ROOT/dist/cd-staging"
DIST="$REPO_ROOT/dist"

echo "▸ Building release binary…"
( cd "$REPO_ROOT" && cargo build --release )

echo "▸ Resolving binary…"
BIN_HOST=""
case "$(uname -s)" in
    Linux*)   BIN_HOST="$REPO_ROOT/target/release/dicom-viewer" ;;
    Darwin*)  BIN_HOST="$REPO_ROOT/target/release/dicom-viewer" ;;
    MINGW*|MSYS*|CYGWIN*) BIN_HOST="$REPO_ROOT/target/release/dicom-viewer.exe" ;;
    *) echo "Unknown OS, expecting target/release/dicom-viewer*"; BIN_HOST="$REPO_ROOT/target/release/dicom-viewer" ;;
esac
[[ -f "$BIN_HOST" ]] || { echo "Built binary not found at $BIN_HOST"; exit 1; }

echo "▸ Cleaning staging dir…"
rm -rf "$STAGE"
mkdir -p "$STAGE/DICOM"

echo "▸ Copying viewer + manifests…"
cp "$BIN_HOST" "$STAGE/"
cp "$DIST/autorun.inf" "$STAGE/"
cp "$DIST/README.txt" "$STAGE/"
cp "$DIST/HELP.txt"   "$STAGE/"

# Optional: ship a macOS .command launcher next to the unix binary so the
# user can double-click from Finder.
if [[ "$(uname -s)" == "Darwin" || "$(uname -s)" == "Linux" ]]; then
  cat > "$STAGE/Run viewer.command" <<'EOF'
#!/usr/bin/env bash
DIR="$(cd "$(dirname "$0")" && pwd)"
exec "$DIR/dicom-viewer" "$DIR/DICOM"
EOF
  chmod +x "$STAGE/Run viewer.command"
fi

# If the caller pointed us at a folder of DICOM files, copy it as DICOM/.
if [[ "${1-}" != "" ]]; then
    SRC_DICOM="$1"
    [[ -d "$SRC_DICOM" ]] || { echo "Not a directory: $SRC_DICOM"; exit 1; }
    echo "▸ Copying study data from $SRC_DICOM …"
    rsync -a "$SRC_DICOM/" "$STAGE/DICOM/"
fi

echo
echo "✔  Staging ready at: $STAGE"
echo
echo "Next:"
echo "  • Drop your DICOM files into  $STAGE/DICOM/"
echo "  • Burn the folder with your CD/DVD writer."
echo "  • Or build an ISO:"
echo "      genisoimage -V DICOM_VIEWER -J -r -o dicom-viewer.iso \"$STAGE\""
echo "      hdiutil makehybrid -iso -joliet -default-volume-name DICOM_VIEWER \\"
echo "         -o dicom-viewer.iso \"$STAGE\"        # macOS"
echo "      mkisofs -V DICOM_VIEWER -J -r -o dicom-viewer.iso \"$STAGE\""
