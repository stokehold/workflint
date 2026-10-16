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

`.gitattributes` turns off line ending conversion here, so both parsers see these exact bytes on every OS.
