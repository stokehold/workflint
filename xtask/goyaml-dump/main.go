// Command goyaml-dump prints the go-yaml node tree of YAML files for
// `cargo xtask yaml-parity`.
//
// It reads file paths from standard input, one per line. For each file it
// prints a line `=== <path>` and then the dump: one line per node of the raw
// tree before alias resolution, in pre-order, indented by two spaces per
// level. A line holds the node kind, its 1-based `line:column`, its anchor as
// `&name`, and then the alias name as `*name` for an alias or the decoded
// value for a scalar, quoted with strconv.Quote. A file without a root node
// prints `empty`. A file that go-yaml rejects prints `error: ` and the error.
//
// It decodes each file as actionlint does in `Parse` (parse.go): with
// yaml.Unmarshal into a yaml.Node.
package main

import (
	"bufio"
	"fmt"
	"os"
	"strconv"
	"strings"

	"go.yaml.in/yaml/v4"
)

func main() {
	out := bufio.NewWriter(os.Stdout)
	paths := bufio.NewScanner(os.Stdin)
	for paths.Scan() {
		path := paths.Text()
		if path == "" {
			continue
		}
		fmt.Fprintf(out, "=== %s\n", path)
		out.WriteString(dumpFile(path))
	}
	if err := paths.Err(); err != nil {
		fmt.Fprintf(os.Stderr, "goyaml-dump: cannot read the paths: %v\n", err)
		os.Exit(1)
	}
	if err := out.Flush(); err != nil {
		fmt.Fprintf(os.Stderr, "goyaml-dump: cannot write the dump: %v\n", err)
		os.Exit(1)
	}
}

func dumpFile(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Sprintf("error: cannot read the file: %v\n", err)
	}
	return dump(data)
}

func dump(data []byte) (result string) {
	defer func() {
		if r := recover(); r != nil {
			result = fmt.Sprintf("error: panic: %v\n", r)
		}
	}()
	var root yaml.Node
	if err := yaml.Unmarshal(data, &root); err != nil {
		var b strings.Builder
		b.WriteString("error: ")
		if pe, ok := err.(*yaml.ParserError); ok {
			fmt.Fprintf(&b, "%d:%d: ", pe.Line, pe.Column)
		}
		b.WriteString(oneLine(err.Error()))
		b.WriteByte('\n')
		return b.String()
	}
	var b strings.Builder
	node := &root
	if node.Kind == yaml.DocumentNode {
		if len(node.Content) == 0 {
			return "empty\n"
		}
		node = node.Content[0]
	}
	if node.Kind == 0 {
		return "empty\n"
	}
	write(&b, node, 0)
	return b.String()
}

func write(b *strings.Builder, n *yaml.Node, depth int) {
	b.WriteString(strings.Repeat("  ", depth))
	b.WriteString(kind(n.Kind))
	fmt.Fprintf(b, " %d:%d", n.Line, n.Column)
	if n.Anchor != "" {
		b.WriteString(" &" + n.Anchor)
	}
	switch n.Kind {
	case yaml.AliasNode:
		b.WriteString(" *" + n.Value)
	case yaml.ScalarNode:
		b.WriteString(" " + strconv.Quote(n.Value))
	}
	b.WriteByte('\n')
	for _, c := range n.Content {
		write(b, c, depth+1)
	}
}

func kind(k yaml.Kind) string {
	switch k {
	case yaml.DocumentNode:
		return "document"
	case yaml.SequenceNode:
		return "sequence"
	case yaml.MappingNode:
		return "mapping"
	case yaml.ScalarNode:
		return "scalar"
	case yaml.AliasNode:
		return "alias"
	}
	return fmt.Sprintf("kind(%d)", k)
}

func oneLine(s string) string {
	return strings.ReplaceAll(s, "\n", `\n`)
}
