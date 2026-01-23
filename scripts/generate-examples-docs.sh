#!/bin/bash
# Generate docs/EXAMPLES.md from examples/README.md files
#
# This script assembles the examples documentation from README files
# in the examples directory. Run it before building the mdBook.
#
# Usage: ./scripts/generate-examples-docs.sh

set -e

OUTPUT="docs/EXAMPLES.md"
EXAMPLES_DIR="examples"

# Start with the main examples README
cat > "$OUTPUT" << 'HEADER'
# Examples

> **Note**: This file is auto-generated from README files in the `examples/` directory.
> Edit those files instead of this one.

HEADER

# Add the main overview (skip the title since we added one)
tail -n +3 "$EXAMPLES_DIR/README.md" >> "$OUTPUT"

echo "" >> "$OUTPUT"
echo "---" >> "$OUTPUT"
echo "" >> "$OUTPUT"

# Process each category in order
CATEGORIES="basics language paradigms data io projects ffi"

for category in $CATEGORIES; do
    readme="$EXAMPLES_DIR/$category/README.md"
    if [ -f "$readme" ]; then
        echo "Processing $readme..."
        cat "$readme" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
        echo "---" >> "$OUTPUT"
        echo "" >> "$OUTPUT"
    fi
done

# Add footer
cat >> "$OUTPUT" << 'FOOTER'
## See Also

- [Language Guide](language-guide.md) - Core language concepts
- [Weaves Guide](WEAVES_GUIDE.md) - Generators and coroutines
- [Testing Guide](TESTING_GUIDE.md) - Writing and running tests
- [seqlings](https://github.com/navicore/seqlings) - Interactive exercises
FOOTER

echo "Generated $OUTPUT"
