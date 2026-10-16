// Prints how Go's strconv.Atoi and strconv.ParseFloat handle each input, for
// the tests of src/strconv.rs. Regenerate the table with
//
//	go run crates/workflint-syntax/tests/data/strconv.go > crates/workflint-syntax/tests/data/strconv.tsv
//
// Each line is the function, the input, and either "ok" and the result or
// "err" and the error text, separated by tabs. A float result is its IEEE 754
// bits in hexadecimal. Inputs must not contain tabs or line breaks.
package main

import (
	"fmt"
	"math"
	"strconv"
)

var atoi = []string{
	"0", "1", "-1", "+1", "-0", "00012", "+", "-", "", " 1", "1 ", "1x",
	"1.5", "1e3", "1_000", "0x10", "0o17", "0b101", "017", "true", "abc", "１",
	"123456789012345678", "1234567890123456789", "12345678901234567890",
	"9223372036854775807", "9223372036854775808", "-9223372036854775808",
	"-9223372036854775809", "99999999999999999999", "99999999999999999999x",
	"-99999999999999999999x", "+9223372036854775807",
}

var parseFloat = []string{
	"0", "1", "-1", "+1", "-0", "-0.0", "00", "017", "1.5", ".5", "1.", "-.5",
	"+.5", "1e3", "1E3", "1e+3", "1e-3", "1e", "1e+", "e3", ".", "-", "+", "",
	" 1", "1 ", "1.5.5", "1,5", "1:20", "2026-10-15", "true", "abc", "１",
	"1_000", "1__0", "_1", "1_", "1_.5", "1._5", "1e1_0", "1e_10", "1_e10",
	"0x10", "0x1p4", "0X1P4", "0x1P-2", "0x1.8p1", "0x.8p1", "0x1", "0x1.8",
	"0x", "0xp1", "0x_1p0", "0x1_0p0", "0x1p", "0x1p+", "0x1e3", "0x1fp0",
	"0o17", "0b101", "1p3",
	"inf", "Inf", "INF", "+inf", "-inf", "infinity", "-Infinity", "+INFINITY",
	"infin", "infx", "in", "nan", "NaN", "NAN", "-nan", "+NaN", "nanx",
	".nan", ".NaN", ".inf", "-.inf", "+.inf", ".Inf",
	"1e308", "1e309", "-1e309", "1.7976931348623157e308",
	"1.7976931348623158e308", "1.7976931348623159e308", "4.9e-324", "2.4e-324",
	"2.5e-324", "1e-400", "0e99999", "1e99999999999", "-1e99999999999",
	"0x1p1023", "0x1p1024", "0x1p-1074", "0x1p-1075", "0x1.8p-1074",
	"0x1.fffffffffffffp1023", "0x1.fffffffffffff8p1023",
	"0x1.00000000000008p0", "0x1.00000000000018p0", "0x1.000000000000081p0",
	"0x123456789abcdef0123p0", "0x0p99999", "0x1p-99999", "0x1p99999",
	"123456789012345678901234567890", "0.1", "0.30000000000000004",
	"9007199254740993", "9007199254740993.0000000000000000000001",
	"2.2250738585072011e-308", "2.2250738585072014e-308",
	"000000000000000000000000000001.5", "1.500000000000000000000000000000",
}

func main() {
	for _, s := range atoi {
		if v, err := strconv.Atoi(s); err != nil {
			fmt.Printf("Atoi\t%s\terr\t%s\n", s, err)
		} else {
			fmt.Printf("Atoi\t%s\tok\t%d\n", s, v)
		}
	}
	for _, s := range parseFloat {
		if v, err := strconv.ParseFloat(s, 64); err != nil {
			fmt.Printf("ParseFloat\t%s\terr\t%s\n", s, err)
		} else {
			fmt.Printf("ParseFloat\t%s\tok\t%016x\n", s, math.Float64bits(v))
		}
	}
}
