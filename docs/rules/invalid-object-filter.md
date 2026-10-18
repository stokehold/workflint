# invalid-object-filter

| Kind | Category | Default severity |
|---|---|---|
| `expression` | correctness | error |

An object filter `.*` applies to a value without object elements.

`.*` takes the elements of an array or the values of an object. A property after it reads that property from each element, so the elements must be objects.

## Why

No element has the property, so the expression never gives a useful value.

## Failing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ toJSON(env.*.name) }}'
```

```text
test.yaml:6:31: elements of object at receiver of object filtering `.*` must be type of object but got "string". the type of receiver was "{string => string}" [invalid-object-filter]
```

## Passing example

```yaml
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: echo '${{ toJSON(github.event.commits.*.id) }}'
```

## Differences from actionlint

- [Object filters do not change shared types](../differences.md#object-filter-mutation)
