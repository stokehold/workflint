# YAML parity inputs

`cargo xtask yaml-parity` compares the tree of every file here, as well as the vendored test data and the corpus, with go-yaml's. These files cover cases that the reference's fixtures lack:

- `bom-crlf.yaml`: a UTF-8 BOM and `\r\n` line breaks.
- `lone-cr.yaml`: lone `\r` line breaks, including inside a block scalar.
- `unicode-line-breaks.yaml`: U+0085, U+2028, and U+2029 between nodes.
- `multi-document.yaml`: three documents, of which only the first is parsed.
- `multi-document-error-in-second.yaml`: a syntax error in the second document, which neither parser reports.
- `eof-*.yaml`: block scalars at the end of the file without a final line break. libyaml-safer 0.3.0 panicked on `eof-literal.yaml`.
- `non-ascii.yaml`: non-ASCII keys and values, including characters outside the Basic Multilingual Plane, a combining accent, and right-to-left text.

`.gitattributes` turns off line ending conversion here, so both parsers see these exact bytes on every OS.
