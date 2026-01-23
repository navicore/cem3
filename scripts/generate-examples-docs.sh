#!/bin/bash
# Generate docs from source files:
#   - docs/README.md from root README.md (with ## converted to bold for mdBook)
#   - docs/EXAMPLES.md from examples/README.md files
#
# Usage: ./scripts/generate-examples-docs.sh
#        just gen-docs

set -e

# Generate docs/README.md from root README.md
# Convert ## headers to bold so they don't appear in mdBook sidebar
echo "Generating docs/README.md..."
sed 's/^## \(.*\)/**\1**/' README.md > docs/README.md

OUTPUT="docs/EXAMPLES.md"
EXAMPLES_DIR="examples"

# Start with minimal header
cat > "$OUTPUT" << 'HEADER'
# Examples

> **Note**: This file is auto-generated from README files in the `examples/` directory.
> Run `just gen-docs` to regenerate, or edit the source README files.

HEADER

# Categories in display order (top-level only)
CATEGORIES="basics language paradigms data io projects ffi"

# Function to adjust header levels
# $1 = number of levels to increase (1 for category, 2 for subcategory)
adjust_headers() {
    local levels=$1
    if [ "$levels" -eq 1 ]; then
        # # -> ##, ## -> ###, etc.
        awk '{
            if (/^####/) { sub(/^####/, "#####"); }
            else if (/^###/) { sub(/^###/, "####"); }
            else if (/^##/) { sub(/^##/, "###"); }
            else if (/^#/) { sub(/^#/, "##"); }
            print
        }'
    else
        # # -> ###, ## -> ####, etc. (for nested subcategories)
        awk '{
            if (/^###/) { sub(/^###/, "#####"); }
            else if (/^##/) { sub(/^##/, "####"); }
            else if (/^#/) { sub(/^#/, "###"); }
            print
        }'
    fi
}

for category in $CATEGORIES; do
    category_readme="$EXAMPLES_DIR/$category/README.md"

    if [ -f "$category_readme" ]; then
        echo "Processing $category_readme..."
        echo "" >> "$OUTPUT"
        cat "$category_readme" | adjust_headers 1 >> "$OUTPUT"
        echo "" >> "$OUTPUT"
    fi

    # Look for subcategory READMEs (one level deep)
    for subdir in "$EXAMPLES_DIR/$category"/*/; do
        if [ -d "$subdir" ]; then
            sub_readme="${subdir}README.md"
            if [ -f "$sub_readme" ]; then
                echo "  Processing $sub_readme..."
                echo "" >> "$OUTPUT"
                cat "$sub_readme" | adjust_headers 2 >> "$OUTPUT"
                echo "" >> "$OUTPUT"
            fi
        fi
    done
done

# Add footer
cat >> "$OUTPUT" << 'FOOTER'
---

## See Also

- [Language Guide](language-guide.md) - Core language concepts
- [Weaves Guide](WEAVES_GUIDE.md) - Generators and coroutines
- [Testing Guide](TESTING_GUIDE.md) - Writing and running tests
- [seqlings](https://github.com/navicore/seqlings) - Interactive exercises
FOOTER

echo "Generated $OUTPUT"
