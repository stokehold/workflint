// Command expr-check runs the reference's expression checker for
// `cargo xtask expr-parity`.
//
// The xtask command compiles it inside the reference checkout with
// `go build -overlay`, as the package `cmd/workflint-expr-check`. It builds
// against the checkout's code and the dependencies in its go.sum, and the
// checkout stays unchanged. This directory has no go.mod for that reason.
//
// Its only argument is a JSON file with the setups and the cases. For each
// case, in order, it prints one line of JSON: the parse error, or the type,
// the result of IsConstant, and the errors of Check. Each error has its
// 1-based line and column.
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"os"

	"github.com/rhysd/actionlint"
)

// typeSpec is a type, as `cargo xtask expr-parity` writes it.
type typeSpec struct {
	// Kind is any, null, number, bool, string, array, strict, loose, or map.
	Kind string `json:"kind"`
	// Props are the properties of an object.
	Props map[string]*typeSpec `json:"props"`
	// Elem is the element type of an array or of a map.
	Elem *typeSpec `json:"elem"`
}

func (s *typeSpec) toType() actionlint.ExprType {
	switch s.Kind {
	case "any":
		return actionlint.AnyType{}
	case "null":
		return actionlint.NullType{}
	case "number":
		return actionlint.NumberType{}
	case "bool":
		return actionlint.BoolType{}
	case "string":
		return actionlint.StringType{}
	case "array":
		return &actionlint.ArrayType{Elem: s.Elem.toType()}
	default:
		return s.toObject()
	}
}

func (s *typeSpec) toObject() *actionlint.ObjectType {
	props := make(map[string]actionlint.ExprType, len(s.Props))
	for name, prop := range s.Props {
		props[name] = prop.toType()
	}
	switch s.Kind {
	case "strict":
		return actionlint.NewStrictObjectType(props)
	case "loose":
		return actionlint.NewObjectType(props)
	case "map":
		return &actionlint.ObjectType{Props: props, Mapped: s.Elem.toType()}
	default:
		fail(fmt.Errorf("%q is not an object type", s.Kind))
		return nil
	}
}

// setup is the environment of the checker for a group of cases.
type setup struct {
	Name           string      `json:"name"`
	Key            string      `json:"key"`
	Untrusted      bool        `json:"untrusted"`
	ConfigVars     []string    `json:"config_vars"`
	Matrix         *typeSpec   `json:"matrix"`
	Steps          *typeSpec   `json:"steps"`
	Needs          *typeSpec   `json:"needs"`
	Secrets        *typeSpec   `json:"secrets"`
	Jobs           *typeSpec   `json:"jobs"`
	Inputs         []*typeSpec `json:"inputs"`
	DispatchInputs *typeSpec   `json:"dispatch_inputs"`
}

// checker returns a new checker for the setup, configured as the reference
// configures one for a workflow key.
func (s *setup) checker() *actionlint.ExprSemanticsChecker {
	c := actionlint.NewExprSemanticsChecker(s.Untrusted, s.ConfigVars)
	if s.Matrix != nil {
		c.UpdateMatrix(s.Matrix.toObject())
	}
	if s.Steps != nil {
		c.UpdateSteps(s.Steps.toObject())
	}
	if s.Needs != nil {
		c.UpdateNeeds(s.Needs.toObject())
	}
	if s.Secrets != nil {
		c.UpdateSecrets(s.Secrets.toObject())
	}
	if s.Jobs != nil {
		c.UpdateJobs(s.Jobs.toObject())
	}
	for _, inputs := range s.Inputs {
		c.UpdateInputs(inputs.toObject())
	}
	if s.DispatchInputs != nil {
		c.UpdateDispatchInputs(s.DispatchInputs.toObject())
	}
	contexts, functions := actionlint.WorkflowKeyAvailability(s.Key)
	c.SetContextAvailability(contexts)
	c.SetSpecialFunctionAvailability(functions)
	return c
}

type input struct {
	Setups []setup `json:"setups"`
	Cases  []struct {
		Setup      int    `json:"setup"`
		Expression string `json:"expression"`
	} `json:"cases"`
}

type message struct {
	Line    int    `json:"line"`
	Column  int    `json:"column"`
	Message string `json:"message"`
}

type outcome struct {
	ParseError *message  `json:"parse_error,omitempty"`
	Type       *string   `json:"type,omitempty"`
	Constant   *bool     `json:"constant,omitempty"`
	Errors     []message `json:"errors,omitempty"`
}

func check(s *setup, expression string) outcome {
	expr, err := actionlint.NewExprParser().Parse(actionlint.NewExprLexer(expression + "}}"))
	if err != nil {
		return outcome{ParseError: &message{err.Line, err.Column, err.Message}}
	}
	c := s.checker()
	ty, errs := c.Check(expr)
	name := ty.String()
	constant := c.IsConstant(expr)
	o := outcome{Type: &name, Constant: &constant}
	for _, e := range errs {
		o.Errors = append(o.Errors, message{e.Line, e.Column, e.Message})
	}
	return o
}

func fail(err error) {
	fmt.Fprintf(os.Stderr, "expr-check: %v\n", err)
	os.Exit(1)
}

func main() {
	if len(os.Args) != 2 {
		fail(fmt.Errorf("usage: expr-check <input.json>"))
	}
	bytes, err := os.ReadFile(os.Args[1])
	if err != nil {
		fail(err)
	}
	var in input
	if err := json.Unmarshal(bytes, &in); err != nil {
		fail(err)
	}

	// Checking `x.*` marks the array type of `x` as filtered in place, and the
	// global context types are shared by every checker
	// (docs/differences.md#object-filter-mutation). Each case starts from a
	// copy of the original types, so that one case cannot change the next.
	pristine := make(map[string]actionlint.ExprType, len(actionlint.BuiltinGlobalVariableTypes))
	for name, ty := range actionlint.BuiltinGlobalVariableTypes {
		pristine[name] = ty.DeepCopy()
	}

	out := bufio.NewWriter(os.Stdout)
	encoder := json.NewEncoder(out)
	encoder.SetEscapeHTML(false)
	for _, c := range in.Cases {
		if c.Setup < 0 || c.Setup >= len(in.Setups) {
			fail(fmt.Errorf("case %q names setup %d of %d", c.Expression, c.Setup, len(in.Setups)))
		}
		for name, ty := range pristine {
			actionlint.BuiltinGlobalVariableTypes[name] = ty.DeepCopy()
		}
		if err := encoder.Encode(check(&in.Setups[c.Setup], c.Expression)); err != nil {
			fail(err)
		}
	}
	if err := out.Flush(); err != nil {
		fail(err)
	}
}
