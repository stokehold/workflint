# YAML parity inputs

`cargo xtask yaml-parity` compares the tree of every file here, as well as the vendored test data and the corpus, with go-yaml's. These files cover cases that the reference's fixtures lack:

- `bom-crlf.yaml`: a UTF-8 BOM and `\r\n` line breaks.
- `lone-cr.yaml`: lone `\r` line breaks, including inside a block scalar.
- `unicode-line-breaks.yaml`: U+0085, U+2028, and U+2029 between nodes.
- `multi-document.yaml`: three documents, of which only the first is parsed.
- `multi-document-error-in-second.yaml`: a syntax error in the second document, which neither parser reports.
- `eof-*.yaml`: block scalars at the end of the file without a final line break. libyaml-safer 0.3.0 panicked on `eof-literal.yaml`.
- `non-ascii.yaml`: non-ASCII keys and values, including characters outside the Basic Multilingual Plane, a combining accent, and right-to-left text.

These files cover cases where go-yaml and libyaml disagree. The vendored parser follows go-yaml there (#50):

- `anchor-names.yaml`: anchor and alias names with `.`, `/`, and `$`.
- `long-key-non-ascii.yaml`: a simple key of 600 two-byte characters, under the limit of 1,024 characters.
- `tab-before-own-line-comment.yaml`: a tab before a comment on the line after another comment.
- `escaped-single-quote.yaml`: the escape `\'` in a double-quoted scalar.
- `flow-colon-and-question-mark.yaml`: `[b:,c]`, `{b:}`, and `[?x]`.
- `bom-after-start.yaml`: a byte order mark at the start of the second line, which stays in the key.
- `control-character-in-second-document.yaml`: a control character after the first document, which go-yaml's reader finds before it parses.
- `error-in-lookahead-*.yaml`: syntax errors in the tokens that go-yaml's scanner reads ahead of the parser, which fail the file.
- `no-error-past-lookahead.yaml`: a syntax error in the second document past that lookahead, which neither parser reports.
- `empty-document.yaml`, `explicit-key-without-value.yaml`, `explicit-key-before-dedent.yaml`: the positions of empty scalars at a document or block end.
- `utf-16le.yaml`, `utf-16be.yaml`: UTF-16 with a byte order mark.
- `depth-limit-flow.yaml`, `depth-limit-block.yaml`: 10,001 flow levels and 10,001 block indentation levels, one more than go-yaml allows.
- `empty-flow-collection-key.yaml`: `[] x: y`, where the parser takes the `[]` before the scanner finds the `:`, so the document is `[]`.
- `unclosed-quoted-scalar.yaml`: a double-quoted scalar without its closing quote.
- `scalar-styles-and-tags.yaml`: every scalar style, and explicit tags in short, verbatim, custom, and non-specific form.

The dumps of these files differ from go-yaml's as an entry of `docs/differences.md` describes. `ALLOWED_DIFFERENCES` in `xtask/src/yaml_parity.rs` names the entry of each, and the command fails if one of them stops differing:

- `document-start-at-eof.yaml`: only `---`, without a final line break (`empty-scalar-past-last-line`).
- `scanner-error-after-comment-before-dash.yaml`: a scanner error that go-yaml ignores after a comment before a `-` item (`scanner-error-after-comment-before-dash`).
- `tag-before-flow-sequence-end.yaml`: `[!!str]` (`tag-before-flow-comma`).
- `utf16le-control-character-in-second-chunk.yaml`: a control character after a syntax error in UTF-16 (`utf16-control-characters`).
- `utf16le-second-byte-order-mark.yaml`: UTF-16 text that starts with a second byte order mark (`utf16-second-bom`).

`.gitattributes` turns off line ending conversion here, so both parsers see these exact bytes on every OS.
