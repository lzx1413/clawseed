#!/usr/bin/env bash
# Push .clawseed/skills/ to an Android device and reload the gateway skill index.
#
# Usage: ./tools/sync-skills-android.sh [PACKAGE_NAME]
#
# Default package: dev.clawseed.demo
# Prerequisites:
#   - adb in PATH, device connected
#   - ClawSeed app running with gateway active
#
# The script copies workspace .clawseed/skills/ to the app's internal
# filesDir/.clawseed/skills/ via run-as, then calls POST /api/skills/reload
# on the gateway so changes take effect immediately.

set -euo pipefail

PACKAGE="${1:-dev.clawseed.demo}"
SKILLS_SRC=".clawseed/skills"
DEVICE_TMP="/data/local/tmp/clawseed-skills"
GATEWAY_PORT=3000

if ! adb get-state >/dev/null 2>&1; then
    echo "ERROR: No Android device connected (adb)"
    exit 1
fi

if [ ! -d "${SKILLS_SRC}" ]; then
    echo "ERROR: ${SKILLS_SRC}/ not found in current directory"
    exit 1
fi

echo "==> Pushing skills to device (package: ${PACKAGE})"

# Push to temp location first (adb push can't write to app-private dirs)
adb push "${SKILLS_SRC}/" "${DEVICE_TMP}/" >/dev/null

# Copy from temp to app internal storage via run-as
# Skills are stored under the workspace directory so the agent can manage them
# via file tools. Workspace path: /data/data/<pkg>/files/.clawseed/workspace/
SKILLS_DIR="/data/data/${PACKAGE}/files/.clawseed/workspace/.clawseed/skills"

adb shell "run-as ${PACKAGE} mkdir -p ${SKILLS_DIR}" 2>/dev/null

# List skill directories in the temp location and copy each one
SKILL_DIRS=$(adb shell "ls ${DEVICE_TMP}" 2>/dev/null | tr -d '\r')
for dir in ${SKILL_DIRS}; do
    echo "    Syncing: ${dir}"
    # Remove old version on device, then copy fresh
    adb shell "run-as ${PACKAGE} rm -rf ${SKILLS_DIR}/${dir}" 2>/dev/null
    adb shell "run-as ${PACKAGE} cp -r ${DEVICE_TMP}/${dir} ${SKILLS_DIR}/${dir}" 2>/dev/null
done

# Clean up temp
adb shell "rm -rf ${DEVICE_TMP}" 2>/dev/null

echo "==> Reloading gateway skill index..."
# Forward gateway port if not already forwarded
adb forward tcp:${GATEWAY_PORT} tcp:${GATEWAY_PORT} 2>/dev/null || true

RESPONSE=$(curl -s -X POST "http://localhost:${GATEWAY_PORT}/api/skills/reload" \
    -H "Content-Type: application/json" \
    -d '{}' 2>/dev/null || echo '{"ok":false}')

if echo "${RESPONSE}" | grep -q '"ok":true'; then
    COUNT=$(echo "${RESPONSE}" | grep -o '"skills_count":[0-9]*' | grep -o '[0-9]*')
    echo "==> Done! ${COUNT} skills loaded."
else
    echo "WARNING: Reload API call failed. Gateway may not be running."
    echo "    Response: ${RESPONSE}"
    echo "    Skills are on device but you'll need to restart the app or use the refresh button."
fi
