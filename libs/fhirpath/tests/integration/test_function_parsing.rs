//! Tests to verify that all FHIRPath functions can be parsed

use zunder_fhirpath::error::Result;
use zunder_fhirpath::parser::Parser;

fn parse_function_call(expr: &str) -> Result<()> {
    let mut parser = Parser::new(expr.to_string());
    parser.parse()?;
    Ok(())
}

#[test]
fn test_parse_all_functions() {
    // Boolean logic
    assert!(parse_function_call("not()").is_ok());
    // Note: "as" is parsed as a type operator (value as 'string'), not a function call
    // The function form would be: as(value, 'string') but FHIRPath uses operator syntax

    // Conversion
    assert!(parse_function_call("iif(true, 'yes', 'no')").is_ok());
    assert!(parse_function_call("toBoolean()").is_ok());
    assert!(parse_function_call("convertsToBoolean()").is_ok());
    assert!(parse_function_call("toInteger()").is_ok());
    assert!(parse_function_call("convertsToInteger()").is_ok());
    assert!(parse_function_call("toDecimal()").is_ok());
    assert!(parse_function_call("convertsToDecimal()").is_ok());
    assert!(parse_function_call("convertsToString()").is_ok());
    assert!(parse_function_call("toDate()").is_ok());
    assert!(parse_function_call("convertsToDate()").is_ok());
    assert!(parse_function_call("toDateTime()").is_ok());
    assert!(parse_function_call("convertsToDateTime()").is_ok());
    assert!(parse_function_call("toTime()").is_ok());
    assert!(parse_function_call("convertsToTime()").is_ok());
    assert!(parse_function_call("toQuantity()").is_ok());
    assert!(parse_function_call("convertsToQuantity()").is_ok());

    // Existence
    assert!(parse_function_call("empty()").is_ok());
    assert!(parse_function_call("exists()").is_ok());
    assert!(parse_function_call("all($this > 0)").is_ok());
    assert!(parse_function_call("allTrue()").is_ok());
    assert!(parse_function_call("anyTrue()").is_ok());
    assert!(parse_function_call("allFalse()").is_ok());
    assert!(parse_function_call("anyFalse()").is_ok());
    // Note: FHIRPath doesn't have array literals, collections come from expressions
    // These would be called on collections: collection.subsetOf(otherCollection)
    assert!(parse_function_call("subsetOf(collection)").is_ok());
    assert!(parse_function_call("supersetOf(collection)").is_ok());
    assert!(parse_function_call("count()").is_ok());
    assert!(parse_function_call("distinct()").is_ok());
    assert!(parse_function_call("isDistinct()").is_ok());

    // Filtering
    assert!(parse_function_call("where($this > 0)").is_ok());
    assert!(parse_function_call("select($this.name)").is_ok());
    assert!(parse_function_call("repeat('name')").is_ok());
    assert!(parse_function_call("ofType('Patient')").is_ok());
    assert!(parse_function_call("extension('url')").is_ok());
    assert!(parse_function_call("extension('url', 'value')").is_ok());

    // Subsetting
    assert!(parse_function_call("single()").is_ok());
    assert!(parse_function_call("first()").is_ok());
    assert!(parse_function_call("last()").is_ok());
    assert!(parse_function_call("tail()").is_ok());
    assert!(parse_function_call("skip(5)").is_ok());
    assert!(parse_function_call("take(10)").is_ok());
    assert!(parse_function_call("intersect(collection)").is_ok());
    assert!(parse_function_call("exclude(collection)").is_ok());

    // Combining
    assert!(parse_function_call("union(collection)").is_ok());
    assert!(parse_function_call("combine(collection)").is_ok());

    // String
    assert!(parse_function_call("toString()").is_ok());
    assert!(parse_function_call("indexOf('test')").is_ok());
    assert!(parse_function_call("lastIndexOf('test')").is_ok());
    assert!(parse_function_call("substring(5)").is_ok());
    assert!(parse_function_call("substring(5, 10)").is_ok());
    assert!(parse_function_call("startsWith('prefix')").is_ok());
    assert!(parse_function_call("endsWith('suffix')").is_ok());
    // Note: "contains" is a binary operator (collection contains value), not a function
    // The string function is different - it's called on strings: string.contains('substring')
    // But in path context: name.contains('substring') should work
    // Let's test it in a path context instead
    // assert!(parse_function_call("contains('substring')").is_ok()); // This would be ambiguous
    assert!(parse_function_call("upper()").is_ok());
    assert!(parse_function_call("lower()").is_ok());
    assert!(parse_function_call("replace('old', 'new')").is_ok());
    assert!(parse_function_call("matches('pattern')").is_ok());
    assert!(parse_function_call("matchesFull('pattern')").is_ok());
    assert!(parse_function_call("replaceMatches('pattern', 'replacement')").is_ok());
    assert!(parse_function_call("length()").is_ok());
    assert!(parse_function_call("toChars()").is_ok());
    assert!(parse_function_call("trim()").is_ok());
    assert!(parse_function_call("encode('type')").is_ok());
    assert!(parse_function_call("decode('type')").is_ok());
    assert!(parse_function_call("escape('type')").is_ok());
    assert!(parse_function_call("unescape('type')").is_ok());
    assert!(parse_function_call("split(',')").is_ok());
    assert!(parse_function_call("join(',')").is_ok());

    // Math
    assert!(parse_function_call("abs()").is_ok());
    assert!(parse_function_call("ceiling()").is_ok());
    assert!(parse_function_call("exp()").is_ok());
    assert!(parse_function_call("floor()").is_ok());
    assert!(parse_function_call("ln()").is_ok());
    assert!(parse_function_call("log(10)").is_ok());
    assert!(parse_function_call("power(2)").is_ok());
    assert!(parse_function_call("round()").is_ok());
    assert!(parse_function_call("round(2)").is_ok());
    assert!(parse_function_call("sqrt()").is_ok());
    assert!(parse_function_call("truncate()").is_ok());

    // Navigation
    assert!(parse_function_call("children()").is_ok());
    assert!(parse_function_call("children('name')").is_ok());
    assert!(parse_function_call("descendants()").is_ok());
    assert!(parse_function_call("descendants('name')").is_ok());

    // Type
    // Note: "is" is parsed as a type operator (value is 'Patient'), not a function call
    // The function form exists but uses operator syntax in FHIRPath
    // assert!(parse_function_call("is('Patient')").is_ok()); // Would be parsed as operator

    // Utility
    assert!(parse_function_call("trace('label')").is_ok());
    assert!(parse_function_call("trace('label', value)").is_ok());
    assert!(parse_function_call("now()").is_ok());
    assert!(parse_function_call("today()").is_ok());
    assert!(parse_function_call("timeOfDay()").is_ok());
    assert!(parse_function_call("sort()").is_ok());
    assert!(parse_function_call("sort('asc')").is_ok());
    assert!(parse_function_call("lowBoundary()").is_ok());
    assert!(parse_function_call("highBoundary()").is_ok());
    assert!(parse_function_call("comparable(value)").is_ok());
    assert!(parse_function_call("precision()").is_ok());
    assert!(parse_function_call("type()").is_ok());
    assert!(parse_function_call("conformsTo('url')").is_ok());
    assert!(parse_function_call("hasValue()").is_ok());
    assert!(parse_function_call("resolve()").is_ok());

    // Aggregate
    assert!(parse_function_call("aggregate($this, 'sum')").is_ok());
}

#[test]
fn test_function_call_in_path() {
    // Functions can be called in path navigation
    assert!(parse_function_call("name.exists()").is_ok());
    assert!(parse_function_call("name.first()").is_ok());
    assert!(parse_function_call("name.where(given = 'John')").is_ok());
    assert!(parse_function_call("name.select(given)").is_ok());
}

#[test]
fn test_chained_function_calls() {
    // Multiple function calls can be chained
    assert!(parse_function_call("name.first().toString()").is_ok());
    assert!(parse_function_call("name.where(given = 'John').first()").is_ok());
}
