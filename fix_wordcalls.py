#!/usr/bin/env python3
import re
import glob

# Pattern to match WordCall structures that need updating
# We need to add suppressed_lints: Vec::new() before the closing brace

def fix_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    original = content

    # Pattern 1: WordCall with span: None, } (no newline before })
    content = re.sub(
        r'Statement::WordCall\s*\{\s*name:\s*([^,]+),\s*span:\s*None,\s*\}',
        r'Statement::WordCall { name: \1, span: None, suppressed_lints: Vec::new(), }',
        content
    )

    # Pattern 2: WordCall with span: Some(...), } (no newline before })
    content = re.sub(
        r'Statement::WordCall\s*\{\s*name:\s*([^,]+),\s*span:\s*(Some\([^)]+\)),\s*\}',
        r'Statement::WordCall { name: \1, span: \2, suppressed_lints: Vec::new(), }',
        content
    )

    # Pattern 3: WordCall with multiline format ending with }], (array context)
    content = re.sub(
        r'Statement::WordCall\s*\{\s*name:\s*([^,]+),\s*span:\s*None,\s*\}\],',
        r'Statement::WordCall { name: \1, span: None, suppressed_lints: Vec::new(), }],',
        content
    )

    if content != original:
        with open(filepath, 'w') as f:
            f.write(content)
        return True
    return False

# Find all .rs files in the crates/compiler directory
files = glob.glob('crates/compiler/src/**/*.rs', recursive=True)

for filepath in files:
    if fix_file(filepath):
        print(f"Updated: {filepath}")
