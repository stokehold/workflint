#!/usr/bin/env bash
# Usage: smoke-test-wheel.sh <wheel> <version>
#
# Installs a PyPI wheel the two ways that docs/releasing.md lists and runs
# smoke-test.sh on each:
#
# - `pip install <wheel>` into a new virtual environment, then the workflint
#   that pip installs.
# - `uvx --from <wheel> workflint`, with uv from requirements-uv.txt.
#
# Neither install reads an index, so the test never fetches workflint from
# PyPI. Needs Python 3 with pip and venv.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: smoke-test-wheel.sh <wheel> <version>" >&2
  exit 2
fi
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
wheel="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
version="$2"

# The python3 of a Windows runner can be a stub that opens the Microsoft
# Store, and a virtual environment on Windows keeps its programs in Scripts.
python=python3
if ! python3 -c '' 2> /dev/null; then
  python=python
fi
bin=bin
if [ "$("$python" -c 'import os; print(os.name)' | tr -d '\r')" = nt ]; then
  bin=Scripts
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

echo "pip install $(basename "$wheel")"
"$python" -m venv "$work/pip"
"$work/pip/$bin/python" -m pip install --quiet --no-index --no-deps "$wheel"
"$here/smoke-test.sh" "$work/pip/$bin/workflint" "$version"

echo "uvx --from $(basename "$wheel") workflint"
"$python" -m venv "$work/uv"
"$work/uv/$bin/python" -m pip install --quiet --no-deps --only-binary :all: --require-hashes \
  -r "$here/requirements-uv.txt"
cat > "$work/uvx-workflint" << EOF
#!/usr/bin/env bash
exec "$work/uv/$bin/uvx" --offline --from "$wheel" workflint "\$@"
EOF
chmod +x "$work/uvx-workflint"
UV_CACHE_DIR="$work/uv-cache" UV_PYTHON_DOWNLOADS=never "$here/smoke-test.sh" "$work/uvx-workflint" "$version"
