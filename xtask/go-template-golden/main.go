// Command go-template-golden writes the expected results that the tests of
// workflint-gotemplate compare with: crates/workflint-gotemplate/tests/golden/cases.rs.in.
//
// It parses and runs each case below with Go's text/template, with
// actionlint's template functions, and writes the template, its output, and
// its parse or execution error as a Rust array of `Case` values. The data sets
// match the ones that crates/workflint-gotemplate/tests/golden.rs builds. Run it
// from the repository root with the Go version in `.go-version`:
//
//	go -C xtask/go-template-golden run . > crates/workflint-gotemplate/tests/golden/cases.rs.in
package main

import (
	"fmt"
	"os"
	"runtime"
	"strings"
	"text/template"
	"unicode"

	"github.com/stokehold/workflint/xtask/go-template-golden/actionlint"
)

// The name that actionlint gives its template.
const name = "error formatter"

type data int

const (
	dataErrors data = iota
	dataReport
	dataNoErrors
	dataMisc
	dataNil
)

var dataNames = map[data]string{
	dataErrors:   "Errors",
	dataReport:   "Report",
	dataNoErrors: "NoErrors",
	dataMisc:     "Misc",
	dataNil:      "Nil",
}

func value(d data) any {
	switch d {
	case dataErrors:
		// testErrorTemplateFields in actionlint's error_test.go.
		return []*actionlint.ErrorTemplateFields{
			{Message: "message 1", Filepath: "file1", Line: 1, Column: 2, EndColumn: 3, Snippet: "snippet 1", Kind: "kind1"},
			{Message: "message 2", Filepath: "file2", Line: 3, Column: 4, EndColumn: 5, Snippet: "snippet 2", Kind: "kind2"},
		}
	case dataReport:
		return []*actionlint.ErrorTemplateFields{
			{
				Message:   `property "platform" is not defined in object type {os: string}`,
				Filepath:  "test.yaml",
				Line:      21,
				Column:    20,
				Kind:      "expression",
				Snippet:   "          key: ${{ matrix.platform }}-node\n                   ^~~~~~~~~~~~~~~",
				EndColumn: 34,
			},
			{
				Message:   "label \"linux-latest\" is unknown. <a> & 'b'\tcafé",
				Filepath:  ".github/workflows/ci.yaml",
				Line:      7,
				Column:    14,
				Kind:      "runner-label",
				Snippet:   "",
				EndColumn: 14,
			},
		}
	case dataNoErrors:
		return []*actionlint.ErrorTemplateFields{}
	case dataMisc:
		return map[string]any{
			"str":     "héllo",
			"int":     42,
			"neg":     -7,
			"zero":    0,
			"float":   3.5,
			"whole":   2.0,
			"big":     1e21,
			"small":   0.00001,
			"t":       true,
			"f":       false,
			"nil":     nil,
			"empty":   "",
			"list":    []any{"a", 1, true, nil},
			"none":    []any{},
			"strs":    []string{"x", "y", "z"},
			"ints":    []int{3, 1, 2},
			"map":     map[string]any{"b": 2, "a": 1, "c": "three"},
			"nomap":   map[string]any{},
			"nested":  map[string]any{"inner": map[string]any{"x": "deep"}},
			"ctrl":    "tab\there\nnl\x00\x07\x7f\r",
			"uni":     "日本語 é \u200b \u00ad \U0001F600 \u2028",
			"quote":   `say "hi" \ back` + "`",
			"html":    `<a href="x">&'</a>` + "\x00",
			"js":      "it's <b> & \"q\" = \\ \u2028 \x01 é",
			"url":     "a b&c=d/é?~_-.",
			"words":   []string{"", "foo", "Foo", "foo-bar", "fooBar", "FooBar", "foo_bar", "foo.bar"},
			"errs":    value(dataErrors),
			"one":     &actionlint.ErrorTemplateFields{Message: "m", Filepath: "f", Line: 1, Column: 2, Kind: "k", Snippet: "s", EndColumn: 3},
			"mixed":   []any{1, "1", 1.0},
			"maps":    []map[string]any{{"k": 1}, {"k": 2}},
			"Capital": "upper",
			"spaced":  "  padded  ",
		}
	default:
		return nil
	}
}

type testCase struct {
	name     string
	data     data
	template string
}

// unescapeBackslash is the function in actionlint's error.go that turns the
// escapes in the -format flag into characters.
func unescapeBackslash(s string) string {
	return strings.NewReplacer(
		`\a`, "\a",
		`\b`, "\b",
		`\f`, "\f",
		`\n`, "\n",
		`\r`, "\r",
		`\t`, "\t",
		`\v`, "\v",
		`\\`, "\\",
	).Replace(s)
}

func recipe(name string, d data, flag string) testCase {
	return testCase{name, d, unescapeBackslash(flag)}
}

func cases() []testCase {
	sarif, err := os.ReadFile("../../tests/conformance/actionlint/testdata/format/sarif_template.txt")
	if err != nil {
		panic(err)
	}
	cs := []testCase{
		// The recipes in actionlint's docs/usage.md and testdata/format/README.md,
		// as the -format flag passes them after unescapeBackslash.
		recipe("recipe_json", dataReport, `{{json .}}`),
		recipe("recipe_json_no_errors", dataNoErrors, `{{json .}}`),
		recipe("recipe_json_lines", dataReport, `{{range $err := .}}{{json $err}}{{end}}`),
		recipe("recipe_markdown", dataReport, "{{range $err := .}}### Error at line {{$err.Line}}, col {{$err.Column}} of `{{$err.Filepath}}`\\n\\n{{$err.Message}}\\n\\n```\\n{{$err.Snippet}}\\n```\\n\\n{{end}}"),
		recipe("recipe_markdown_dollar", dataReport, "{{range $ := .}}### Error at line {{$.Line}}, col {{$.Column}} of `{{$.Filepath}}`\\n\\n{{$.Message}}\\n\\n```\\n{{$.Snippet}}\\n```\\n\\n{{end}}"),
		recipe("recipe_github_annotations", dataReport, "{{range $err := .}}::error file={{$err.Filepath}},line={{$err.Line}},col={{$err.Column}}::{{$err.Message}}%0A```%0A{{replace $err.Snippet \"\\\\n\" \"%0A\"}}%0A```\\n{{end}}"),
		recipe("recipe_printf_q", dataReport, `line is {{$err := index . 0}}{{$err.Line}}, col is {{$err.Column}}, message is {{$err.Message | printf "%q"}}`),
		recipe("recipe_all_kinds", dataNoErrors, `{{range $ = allKinds}}{{$.Name}}{{end}}`),
		recipe("recipe_get_version", dataNoErrors, `{{getVersion}}`),
		recipe("recipe_to_pascal_case", dataReport, `{{toPascalCase (index . 1).Kind}}`),
		// The examples in the tables of docs/usage.md, which are template text.
		{"recipe_table_fields", dataReport, "{{range $err := .}}{{$err.Message}}|{{$err.Snippet}}|{{$err.Kind}}|{{$err.Filepath}}|{{$err.Line}}|{{$err.Column}}|{{$err.EndColumn}}\n{{end}}"},
		{"recipe_table_functions", dataReport, `{{range $err := .}}{{json $err}}{{replace $err.Filepath "\\" "/"}}{{toPascalCase $err.Kind}}{{end}}{{range $ = allKinds}}{{$.Name}}{{end}}{{getVersion}}`},
		{"recipe_table_kinds", dataNoErrors, "{{range $kind := allKinds}}{{$kind.Name}}: {{$kind.Description}}\n{{end}}"},
		{"recipe_sarif", dataReport, string(sarif)},
		{"recipe_sarif_no_errors", dataNoErrors, string(sarif)},

		// The templates in actionlint's error_test.go.
		{"error_test_index_message", dataErrors, "{{(index . 0).Message}} {{(index . 1).Message}}"},
		{"error_test_range_line_column", dataErrors, "{{range $ = .}}({{$.Line}}, {{$.Column}}){{end}}"},
		{"error_test_range_end_column", dataErrors, "{{range $ = .}}[{{$.Column}}..{{$.EndColumn}}]{{end}}"},
		{"error_test_range_json_snippet", dataErrors, "{{range $ = .}}{{json $.Snippet}}{{end}}"},
		{"error_test_range_replace", dataErrors, "{{range $ = .}}{{replace $.Kind \"kind\" \"king\"}}{{end}}"},
		{"error_test_printf_q_pipeline", dataErrors, "{{range $ = .}}({{$.Message | printf \"%q\"}},{{$.Snippet | printf \"%q\"}}){{end}}"},
		{"error_test_all_kinds", dataNoErrors, "{{range $ = allKinds}}{{$.Name}}: {{$.Description}}\n{{end}}"},
		{"error_test_to_pascal_case", dataMisc, "{{range $ = .words}}[{{toPascalCase .}}]{{end}}"},
		{"error_test_missing_field", dataErrors, "{{.Foo}}"},
		{"error_test_json", dataErrors, "{{json .}}"},
		{"error_test_no_placeholder", dataErrors, "hello"},
		{"error_test_unclosed_action", dataErrors, "{{xxx"},

		// Text, actions, and trim markers.
		{"text_only", dataNil, "plain text"},
		{"empty_template", dataNil, ""},
		{"dot_nil", dataNil, "{{.}}"},
		{"trim_left", dataNil, "a  \n\t {{- 1}}"},
		{"trim_right", dataNil, "{{1 -}} \n\t b"},
		{"trim_both", dataNil, "a \n {{- 1 -}} \n b"},
		{"trim_needs_space", dataNil, "a {{-1}} b"},
		{"trim_minus_number", dataNil, "{{- -1 -}}"},
		{"trim_comment", dataNil, "a \n {{- /* comment */ -}} \n b"},
		{"comment", dataNil, "a{{/* a\ncomment */}}b"},
		{"comment_unclosed", dataNil, "a{{/* comment"},
		{"comment_before_delimiter", dataNil, "a{{/* comment */ 1}}"},
		{"newline_in_action", dataNil, "{{print\n1\n2}}"},
		{"raw_string", dataNil, "{{`a\\n\"b`}}"},
		{"string_escapes", dataNil, `{{"\t\x41\u00e9\U0001F600\101\\\""}}`},
		{"char_constant", dataNil, `{{'a'}} {{'\n'}} {{printf "%c" 'é'}}`},
		{"numbers", dataNil, "{{1}} {{-2}} {{+3}} {{1.5}} {{1e3}} {{0x1F}} {{0o17}} {{0b101}} {{1_000}} {{0x1p4}} {{.5}}"},
		{"number_overflow", dataNil, "{{9223372036854775808}}"},
		{"number_bad", dataNil, "{{1x}}"},
		{"number_illegal", dataNil, "{{08}}"},
		{"float_formats", dataMisc, "{{.float}} {{.whole}} {{.big}} {{.small}} {{1e20}} {{1e21}} {{0.0001}} {{123456789.0}}"},
		{"bools_and_nil_args", dataNil, "{{true}} {{false}} {{print nil}}"},
		{"nil_command", dataNil, "{{nil}}"},

		// Fields, maps, and structs.
		{"map_fields", dataMisc, "{{.str}} {{.int}} {{.t}} {{.nested.inner.x}}"},
		{"map_missing_key", dataMisc, "[{{.missing}}] [{{.missing.deeper}}]"},
		{"map_nil_value", dataMisc, "[{{.nil}}]"},
		{"print_composites", dataMisc, "{{.list}} {{.none}} {{.strs}} {{.map}} {{.nomap}} {{.one}} {{.nested}} {{.maps}}"},
		{"struct_field", dataMisc, "{{.one.Message}} {{.one.Line}}"},
		{"struct_missing_field", dataMisc, "{{.one.Nope}}"},
		{"field_of_string", dataMisc, "{{.str.Len}}"},
		{"field_of_list", dataErrors, "{{.Message}}"},
		{"field_with_args", dataMisc, "{{.str 1}}"},
		{"map_field_with_args", dataMisc, "{{.nested.inner 1}}"},
		{"chain_on_pipe", dataErrors, "{{(index . 1).Kind}}"},
		{"chain_on_pipe_missing_field", dataErrors, "{{(index . 1).Nope}}"},
		{"chain_on_function", dataNil, "{{getVersion.Nope}}"},
		{"chain_on_literal", dataNil, "{{\"x\".Foo}}"},
		{"chain_on_nil_value", dataMisc, "{{(.nil).Foo}}"},
		{"variable_field_chain", dataMisc, "{{$x := .nested}}{{$x.inner.x}}"},
		{"dollar_is_data", dataMisc, "{{with .nested}}{{$.str}}{{end}}"},
		{"dollar_field_on_nil", dataNil, "[{{$.Foo}}]"},

		// Variables.
		{"variable_declare", dataNil, "{{$x := 1}}{{$x}}"},
		{"variable_reassign", dataNil, "{{$x := 1}}{{$x = 2}}{{$x}}"},
		{"variable_reassign_in_range", dataMisc, "{{$first := true}}{{range .strs}}{{if $first}}{{$first = false}}{{else}},{{end}}{{.}}{{end}}"},
		{"variable_redeclare", dataNil, "{{$x := 1}}{{$x := 2}}{{$x}}"},
		{"variable_scope_ends", dataNil, "{{if true}}{{$x := 1}}{{end}}{{$x}}"},
		{"variable_outer_assign_from_inner", dataNil, "{{$x := 1}}{{if true}}{{$x = 2}}{{end}}{{$x}}"},
		{"variable_undefined", dataNil, "{{$y}}"},
		{"variable_assign_undefined", dataNil, "{{$y = 1}}"},
		{"variable_decl_prints_nothing", dataNil, "a{{$x := 1}}b"},
		{"variable_as_function", dataNil, "{{$x := 1}}{{$x 2}}"},
		{"variable_too_many", dataNil, "{{$a, $b := 1}}"},
		{"variable_in_parens", dataNil, "{{($x := 3)}} {{$x}}"},

		// range.
		{"range_list", dataMisc, "{{range .strs}}<{{.}}>{{end}}"},
		{"range_index_elem", dataMisc, "{{range $i, $e := .strs}}{{$i}}={{$e}} {{end}}"},
		{"range_elem_only", dataMisc, "{{range $e := .strs}}{{$e}}{{end}}"},
		{"range_assign_one", dataMisc, "{{$e := 0}}{{range $e = .strs}}{{end}}{{$e}}"},
		{"range_assign_two", dataMisc, "{{$i := 0}}{{$e := 0}}{{range $i, $e = .strs}}{{end}}{{$i}}{{$e}}"},
		{"range_dollar_assign", dataMisc, "{{range $ = .strs}}{{$}}{{end}}|{{$}}"},
		{"range_map", dataMisc, "{{range $k, $v := .map}}{{$k}}:{{$v}} {{end}}"},
		{"range_map_values", dataMisc, "{{range .map}}{{.}} {{end}}"},
		{"range_else_empty", dataMisc, "{{range .none}}x{{else}}empty{{end}}"},
		{"range_else_nil", dataMisc, "{{range .nil}}x{{else}}nil{{end}}"},
		{"range_else_missing", dataMisc, "{{range .missing}}x{{else}}missing{{end}}"},
		{"range_int", dataNil, "{{range 3}}{{.}}{{end}}"},
		{"range_int_two_vars", dataNil, "{{range $i, $e := 3}}{{end}}"},
		{"range_string", dataMisc, "{{range .str}}{{.}}{{end}}"},
		{"range_bool", dataMisc, "{{range .t}}{{.}}{{end}}"},
		{"range_break", dataMisc, "{{range .strs}}{{if eq . \"y\"}}{{break}}{{end}}{{.}}{{end}}"},
		{"range_continue", dataMisc, "{{range .strs}}{{if eq . \"y\"}}{{continue}}{{end}}{{.}}{{end}}"},
		{"range_nested_break", dataMisc, "{{range .strs}}{{range $.ints}}{{break}}{{end}}{{.}}{{end}}"},
		{"break_outside_range", dataNil, "{{break}}"},
		{"continue_outside_range", dataNil, "{{continue}}"},
		{"break_with_args", dataMisc, "{{range .strs}}{{break 1}}{{end}}"},
		{"range_variables_reset", dataMisc, "{{range .strs}}{{$x := .}}{{end}}{{$x}}"},
		{"range_bad_decl", dataNil, "{{range $a, 1}}{{end}}"},
		{"range_three_vars", dataNil, "{{range $a, $b, $c := .}}{{end}}"},

		// if, else if, with, else with.
		{"if_true", dataNil, "{{if true}}yes{{end}}"},
		{"if_else", dataMisc, "{{if .empty}}yes{{else}}no{{end}}"},
		{"if_else_if", dataMisc, "{{if .f}}a{{else if .zero}}b{{else if .str}}c{{else}}d{{end}}"},
		{"if_truth", dataMisc, "{{if .list}}1{{end}}{{if .none}}2{{end}}{{if .map}}3{{end}}{{if .nomap}}4{{end}}{{if .nil}}5{{end}}{{if .float}}6{{end}}{{if .one}}7{{end}}{{if .missing}}8{{end}}"},
		{"if_missing_value", dataNil, "{{if}}x{{end}}"},
		{"if_unclosed", dataNil, "{{if true}}x"},
		{"if_two_elses", dataNil, "{{if true}}a{{else}}b{{else}}c{{end}}"},
		{"with", dataMisc, "{{with .nested.inner}}{{.x}}{{end}}"},
		{"with_else", dataMisc, "{{with .empty}}x{{else}}{{.str}}{{end}}"},
		{"with_else_with", dataMisc, "{{with .empty}}a{{else with .str}}{{.}}{{else}}c{{end}}"},
		{"with_declare", dataMisc, "{{with $x := .str}}{{$x}}{{end}}"},
		{"end_at_top", dataNil, "a{{end}}"},
		{"else_at_top", dataNil, "a{{else}}"},
		{"end_with_args", dataNil, "{{if true}}{{end 1}}"},

		// define, template, and block.
		{"define_and_template", dataMisc, `{{define "item"}}<{{.}}>{{end}}{{range .strs}}{{template "item" .}}{{end}}`},
		{"template_without_pipeline", dataMisc, `{{define "t"}}[{{.}}]{{end}}{{template "t"}}`},
		{"template_dollar_is_its_dot", dataMisc, `{{define "t"}}{{$.x}}{{end}}{{template "t" .nested.inner}}`},
		{"template_defined_after_use", dataNil, `{{template "t" 1}}{{define "t"}}t{{.}}{{end}}`},
		{"template_not_defined", dataNil, `a{{template "nope"}}`},
		{"template_name_not_string", dataNil, `{{template nope}}`},
		{"template_raw_string_name", dataNil, "{{define `t`}}x{{end}}{{template `t`}}"},
		{"template_calls_template", dataMisc, `{{define "a"}}a{{template "b" .}}{{end}}{{define "b"}}b{{.str}}{{end}}{{template "a" .}}`},
		{"template_exec_error_position", dataMisc, "{{define \"t\"}}\n  {{.one.Nope}}{{end}}{{template \"t\" .}}"},
		{"template_pipeline_declares", dataNil, `{{define "t"}}{{.}}{{end}}{{template "t" $x := 3}}{{$x}}`},
		{"define_cannot_see_variables", dataNil, `{{$x := 1}}{{define "t"}}{{$x}}{{end}}`},
		{"define_cannot_break", dataMisc, `{{range .strs}}{{end}}{{define "t"}}{{break}}{{end}}`},
		{"define_not_at_top_level", dataNil, `{{if true}}{{define "t"}}{{end}}{{end}}`},
		{"define_unclosed", dataNil, `{{define "t"}}x`},
		{"define_with_else", dataNil, `{{define "t"}}x{{else}}{{end}}`},
		{"define_without_name", dataNil, `{{define}}{{end}}`},
		{"define_bad_name", dataNil, `{{define "\q"}}{{end}}`},
		{"define_twice", dataNil, `{{define "t"}}a{{end}}{{define "t"}}b{{end}}`},
		{"define_twice_once_empty", dataNil, `{{define "t"}}a{{end}}{{define "t"}} {{end}}{{template "t"}}`},
		{"define_main_template", dataNil, `{{define "error formatter"}}defined{{end}}`},
		{"define_main_template_twice", dataNil, `x{{define "error formatter"}}defined{{end}}`},
		{"block", dataMisc, `{{block "b" .str}}[{{.}}]{{end}}|{{template "b" "again"}}`},
		{"block_redefined", dataMisc, `{{block "b" .}}default{{end}}{{define "b"}}custom{{end}}`},
		{"block_without_pipeline", dataNil, `{{block "b"}}x{{end}}`},
		{"block_unclosed", dataNil, `{{block "b" .}}x`},

		// Pipelines and parentheses.
		{"pipeline", dataMisc, "{{.str | printf \"%s!\" | printf \"%q\"}}"},
		{"pipeline_literal_stage", dataNil, "{{1 | 2}}"},
		{"parens", dataNil, "{{print (print 1 2) (len \"abc\")}}"},
		{"parens_unclosed", dataNil, "{{print (1}}"},
		{"parens_extra_close", dataNil, "{{print 1)}}"},
		{"pipe_into_non_function", dataMisc, "{{1 | .str}}"},
		{"empty_action", dataNil, "{{}}"},
		{"empty_command", dataNil, "{{print | }}"},
		{"unknown_function", dataNil, "{{nope 1}}"},
		{"bad_character", dataNil, "{{print #}}"},
		{"unrecognized_character", dataNil, "{{print \u00a7}}"},
		{"bad_identifier_char", dataNil, "{{print a#b}}"},
		{"unterminated_string", dataNil, "{{\"abc}}"},
		{"unterminated_raw_string", dataNil, "{{`abc}}"},
		{"unterminated_char", dataNil, "{{'a}}"},
		{"bad_char_constant", dataNil, "{{'ab'}}"},
		{"bad_string_escape", dataNil, "{{\"\\q\"}}"},
		{"expected_declare", dataNil, "{{$x : 1}}"},
		{"unexpected_long_token", dataNil, "{{if true}}{{else \"a very long string\"}}{{end}}"},
		{"unexpected_token_in_operand", dataNil, "{{print 1=}}"},
		{"unclosed_action_multiline", dataNil, "{{print\n1"},
		{"unclosed_action_eof_in_paren", dataNil, "line1\n{{print (1\n"},
		{"dot_number", dataNil, "{{.5.Foo}}"},
		{"keyword_field", dataMisc, "{{.range}}"},
		{"multiline_error_position", dataMisc, "a\nb {{.str}}\n  {{.one.Nope}}"},
		{"exec_error_partial_output", dataMisc, "before {{.str}} {{.one.Nope}} after"},
		{"percent_in_name_context", dataMisc, "{{index .map \"%d\"}}"},

		// Built-in functions.
		{"and_or", dataMisc, "{{and 1 2}} {{and 0 2}} {{or 0 \"\"}} {{or 0 3}} {{and .t .str}} {{or .missing .str}}"},
		{"and_short_circuit", dataMisc, "{{and false (index .list 10)}} {{or true (index .list 10)}}"},
		{"and_pipeline", dataNil, "{{1 | and 2}} {{0 | or \"\"}}"},
		{"and_no_args", dataNil, "{{and}}"},
		{"not", dataMisc, "{{not .t}} {{not .empty}} {{not .none}} {{not .nil}}"},
		{"not_wrong_args", dataNil, "{{not 1 2}}"},
		{"len", dataMisc, "{{len .str}} {{len .list}} {{len .map}} {{len .none}} {{len \"日本\"}}"},
		{"len_of_int", dataMisc, "{{len .int}}"},
		{"len_of_nil", dataMisc, "{{len .nil}}"},
		{"len_no_args", dataNil, "{{len}}"},
		{"index", dataMisc, "{{index .strs 1}} {{index .map \"c\"}} {{index .nested \"inner\" \"x\"}} {{index .str 0}} [{{index .map \"nope\"}}] {{index .strs}}"},
		{"index_out_of_range", dataMisc, "{{index .strs 3}}"},
		{"index_negative", dataMisc, "{{index .strs -1}}"},
		{"index_with_string", dataMisc, "{{index .strs \"a\"}}"},
		{"index_map_with_int", dataMisc, "{{index .map 1}}"},
		{"index_of_int", dataMisc, "{{index .int 1}}"},
		{"index_of_nil", dataMisc, "{{index .nil 1}}"},
		{"index_with_nil", dataMisc, "{{index .strs nil}}"},
		{"slice", dataMisc, "{{slice .strs 1}} {{slice .strs 1 2}} {{slice .strs}} {{slice .str 1 3}} {{slice .strs 0 1 2}}"},
		{"slice_too_many", dataMisc, "{{slice .strs 0 1 2 3}}"},
		{"slice_inverted", dataMisc, "{{slice .strs 2 1}}"},
		{"slice_string_three", dataMisc, "{{slice .str 0 1 2}}"},
		{"slice_of_map", dataMisc, "{{slice .map 0}}"},
		{"slice_out_of_range", dataMisc, "{{slice .strs 0 4}}"},
		{"print", dataMisc, "{{print}}|{{print 1 2}}|{{print \"a\" \"b\"}}|{{print 1 \"a\" 2}}|{{print .list .nil}}|{{print .missing}}"},
		{"println", dataMisc, "{{println}}|{{println 1 \"a\" .t}}|"},
		{"html", dataMisc, "{{html .html}} {{html 1 \"<\"}} {{.html | html}}"},
		{"js", dataMisc, "{{js .js}} {{js .ctrl}}"},
		{"urlquery", dataMisc, "{{urlquery .url}} {{urlquery 1 2}}"},
		{"eq", dataMisc, "{{eq 1 1}} {{eq 1 2}} {{eq \"a\" \"a\"}} {{eq .int 42}} {{eq 1 2 3 1}} {{eq .nil .nil}} {{eq .t true}} {{eq 1.5 1.5}}"},
		{"eq_incompatible", dataMisc, "{{eq 1 \"1\"}}"},
		{"eq_int_float", dataMisc, "{{eq 1 1.0}}"},
		{"eq_missing_arg", dataNil, "{{eq 1}}"},
		{"eq_lists", dataMisc, "{{eq .list .list}}"},
		{"eq_nil_and_list", dataMisc, "{{eq .nil .list}}"},
		{"ne", dataMisc, "{{ne 1 2}} {{ne \"a\" \"a\"}}"},
		{"lt_le_gt_ge", dataNil, "{{lt 1 2}} {{le 2 2}} {{gt 1 2}} {{ge 2 2}} {{lt \"a\" \"b\"}} {{lt 1.5 2.5}} {{gt \"b\" \"a\"}}"},
		{"lt_bool", dataNil, "{{lt true false}}"},
		{"lt_incompatible", dataNil, "{{lt 1 \"a\"}}"},
		{"lt_list", dataMisc, "{{lt .list 1}}"},
		{"lt_wrong_args", dataNil, "{{lt 1}}"},
		{"call_non_function", dataMisc, "{{call .str}}"},
		{"call_nil", dataMisc, "{{call .nil}}"},

		// printf.
		{"printf_q_strings", dataMisc, "{{printf \"%q\" .str}} {{printf \"%q\" .ctrl}} {{printf \"%q\" .uni}} {{printf \"%q\" .quote}}"},
		{"printf_q_flags", dataMisc, "{{printf \"%+q\" .uni}} {{printf \"%#q\" .str}} {{printf \"%#q\" .quote}} {{printf \"%#+q\" .uni}}"},
		{"printf_q_other", dataMisc, "{{printf \"%q\" 65}} {{printf \"%q\" .strs}} {{printf \"%q\" .t}} {{printf \"%q\" .map}}"},
		{"printf_v", dataMisc, "{{printf \"%v|%v|%v|%v|%v|%v|%v\" .str .int .float .t .nil .list .map}}"},
		{"printf_plus_v", dataMisc, "{{printf \"%+v\" .one}} {{printf \"%#v\" .str}} {{printf \"%#v\" .int}}"},
		{"printf_d", dataMisc, "{{printf \"%d %5d %-5d| %05d %+d %x %X %o %b %c %U\" .int .int .int .neg .int 255 255 8 5 65 233}}"},
		{"printf_s", dataMisc, "{{printf \"%s|%10s|%-10s|%.2s|%x|% x|%X\" .str .str .str .str \"hi\" \"hi\" \"hi\"}}"},
		{"printf_f", dataMisc, "{{printf \"%f %.2f %8.3f %e %g %G %.3g %E\" .float .float .float .big .small .big .float 1234.5678}}"},
		{"printf_t", dataMisc, "{{printf \"%t %t\" .t .f}}"},
		{"printf_percent", dataNil, "{{printf \"100%%\"}}"},
		{"printf_bad_verb", dataMisc, "{{printf \"%z %d\" .str .str}}"},
		{"printf_missing", dataNil, "{{printf \"%d %s\" 1}}"},
		{"printf_extra", dataNil, "{{printf \"%d\" 1 \"a\" true}}"},
		{"printf_nil", dataMisc, "{{printf \"%v %s %d\" .nil .nil .nil}}"},
		{"printf_lists", dataMisc, "{{printf \"%d %s %v %x\" .ints .strs .list .strs}}"},
		{"printf_no_verbs", dataNil, "{{printf \"plain\"}}"},
		{"printf_trailing_percent", dataNil, "{{printf \"a%\"}}"},
		{"printf_star_width", dataNil, "{{printf \"%*d|%-*d|%.*f\" 5 1 3 2 1 3.14159}}"},
		{"printf_arg_index", dataNil, "{{printf \"%[2]d %[1]d %d\" 1 2}}"},
		{"printf_format_not_string", dataNil, "{{printf 1}}"},
		{"printf_format_from_field", dataMisc, "{{printf .int}}"},
		{"printf_width_on_q", dataMisc, "{{printf \"%12q|%-12q|\" \"a\" \"b\"}}"},
		{"printf_float_specials", dataNil, "{{printf \"%v %.1f %5.1f %-8.2e|\" 0.1 0.05 -2.25 123456.0}}"},
		{"printf_x_negative", dataNil, "{{printf \"%x %#x %#o %#b %08.3f %+.2e\" -255 255 8 5 3.14159 12345.678}}"},
		{"printf_string_precision_unicode", dataMisc, "{{printf \"%.3s|%5.1s|\" .uni .uni}}"},
		{"printf_c_and_U", dataNil, "{{printf \"%c%c %U %#U\" 26085 0x1F600 0x1F600 0x41}}"},
		{"printf_q_int_invalid_rune", dataNil, "{{printf \"%q %+q\" -1 0x1F600}}"},
		{"printf_s_struct", dataMisc, "{{printf \"%s|%d\" .one .one}}"},
		{"printf_v_nested_map", dataMisc, "{{printf \"%v %+v\" .nested .maps}}"},
		{"printf_v_mixed", dataMisc, "{{printf \"%v %d %s\" .mixed .mixed .mixed}}"},
		{"printf_sharp_v_list", dataMisc, "{{printf \"%#v\" .strs}}"},
		{"printf_space_flag", dataNil, "{{printf \"% d|% d|%+ d\" 5 -5 5}}"},
	}
	return cs
}

// rust writes s as a Rust string literal.
func rust(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		switch {
		case r == '"':
			b.WriteString(`\"`)
		case r == '\\':
			b.WriteString(`\\`)
		case r == '\n':
			b.WriteString(`\n`)
		case r == '\t':
			b.WriteString(`\t`)
		case r == '\r':
			b.WriteString(`\r`)
		case r == unicode.ReplacementChar:
			b.WriteString(`\u{fffd}`)
		case r < ' ' || r == 0x7f || !unicode.IsPrint(r):
			fmt.Fprintf(&b, `\u{%x}`, r)
		default:
			b.WriteRune(r)
		}
	}
	b.WriteByte('"')
	return b.String()
}

func main() {
	var out strings.Builder
	fmt.Fprintf(&out, "// Generated by xtask/go-template-golden with %s. Do not edit.\n[\n", goVersion())
	for _, c := range cases() {
		var want string
		t, err := template.New(name).Funcs(actionlint.Funcs()).Parse(c.template)
		if err != nil {
			want = fmt.Sprintf("Want::ParseError(%s)", rust(err.Error()))
		} else {
			var b strings.Builder
			if err := t.Execute(&b, value(c.data)); err != nil {
				want = fmt.Sprintf("Want::ExecError(%s, %s)", rust(b.String()), rust(err.Error()))
			} else {
				want = fmt.Sprintf("Want::Output(%s)", rust(b.String()))
			}
		}
		fmt.Fprintf(&out, "    Case {\n        name: %s,\n        data: Data::%s,\n        template: %s,\n        want: %s,\n    },\n",
			rust(c.name), dataNames[c.data], rust(c.template), want)
	}
	out.WriteString("]\n")
	os.Stdout.WriteString(out.String())
}

func goVersion() string { return runtime.Version() }
