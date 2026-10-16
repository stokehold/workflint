// Command go-quote-hash prints the hashes that the tests of
// `workflint_diagnostics::quote` compare `quote_rune` and `quote` with.
//
// For every Unicode scalar value from U+0000 to U+10FFFF, surrogates excluded,
// it feeds the output of strconv.QuoteRune and a newline into one 64-bit
// FNV-1a hash, and the output of strconv.Quote on the one-character string and
// a newline into another. It prints the Go and Unicode versions and the two
// hashes in hexadecimal. Run it from the repository root with the Go version
// in `.go-version`:
//
//	go -C xtask/go-quote-hash run .
package main

import (
	"fmt"
	"hash/fnv"
	"runtime"
	"strconv"
	"unicode"
	"unicode/utf8"
)

func main() {
	quoteRune := fnv.New64a()
	quote := fnv.New64a()
	for r := rune(0); r <= unicode.MaxRune; r++ {
		if !utf8.ValidRune(r) {
			continue
		}
		// Writing to a hash never fails.
		_, _ = quoteRune.Write([]byte(strconv.QuoteRune(r) + "\n"))
		_, _ = quote.Write([]byte(strconv.Quote(string(r)) + "\n"))
	}
	fmt.Printf("go version: %s\n", runtime.Version())
	fmt.Printf("unicode version: %s\n", unicode.Version)
	fmt.Printf("QuoteRune: %016x\n", quoteRune.Sum64())
	fmt.Printf("Quote: %016x\n", quote.Sum64())
}
