// Package actionlint holds copies of the types that actionlint passes to its
// -format templates, so that Go names their types as it does in actionlint:
// *actionlint.ErrorTemplateFields and *actionlint.ruleTemplateFields.
package actionlint

import (
	"encoding/json"
	"fmt"
	"strings"
)

// ErrorTemplateFields is a copy of the type in actionlint's error.go.
type ErrorTemplateFields struct {
	Message   string `json:"message"`
	Filepath  string `json:"filepath,omitempty"`
	Line      int    `json:"line"`
	Column    int    `json:"column"`
	Kind      string `json:"kind"`
	Snippet   string `json:"snippet,omitempty"`
	EndColumn int    `json:"end_column"`
}

type ruleTemplateFields struct {
	Name        string
	Description string
}

// Funcs returns actionlint's template functions from error.go, with a fixed
// version and a fixed list of kinds.
func Funcs() map[string]any {
	return map[string]any{
		"json": func(data any) (string, error) {
			var b strings.Builder
			enc := json.NewEncoder(&b)
			if err := enc.Encode(data); err != nil {
				return "", fmt.Errorf("could not encode template value into JSON: %w", err)
			}
			return b.String(), nil
		},
		"replace": func(s string, oldnew ...string) string {
			return strings.NewReplacer(oldnew...).Replace(s)
		},
		"toPascalCase": toPascalCase,
		"getVersion":   func() string { return "1.2.3" },
		"allKinds": func() []*ruleTemplateFields {
			return []*ruleTemplateFields{
				{"expression", "Checks for syntax errors in expressions ${{ }} syntax"},
				{"syntax-check", "Checks for GitHub Actions workflow syntax"},
			}
		},
	}
}

func toPascalCase(s string) string {
	ss := strings.FieldsFunc(s, func(r rune) bool {
		return !('a' <= r && r <= 'z' || 'A' <= r && r <= 'Z' || '0' <= r && r <= '9')
	})
	for i, s := range ss {
		var c rune
		for _, c = range s {
			break
		}
		if 'a' <= c && c <= 'z' {
			ss[i] = strings.ToUpper(s[:1]) + s[1:]
		}
	}
	return strings.Join(ss, "")
}
