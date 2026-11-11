"""Generate the ~1 MB reference doc for bench/acceptance: 10 mermaid, 50 code blocks, wide tables."""
import sys
out = []
para = ("Lorem ipsum dolor sit amet, **consectetur** adipiscing elit, sed do *eiusmod* tempor "
        "incididunt ut labore et `dolore` magna aliqua. Ut enim ad [minim](https://example.com) veniam. ")
mermaid = ["flowchart TD\n  A[Start] --> B{Q}\n  B -->|Yes| C[OK]\n  B -->|No| D[End]",
           "sequenceDiagram\n  Alice->>Bob: hi\n  Bob-->>Alice: hey"]
code = 'fn f(x: u32) -> u32 {\n    // comment\n    let y = x * 2;\n    if y > 10 { y - 1 } else { y + 1 }\n}\n'
i = 0
while sum(map(len, out)) < 1_000_000:
    out.append(f"## Section {i}\n\n" + (para * 4) + "\n\n- item one\n- item **two**\n  - nested\n\n")
    if i % 20 == 0 and i // 20 < 10:
        out.append("```mermaid\n" + mermaid[(i // 20) % 2] + "\n```\n\n")
    if i % 4 == 0 and i // 4 < 50:
        out.append("```rust\n" + code + "```\n\n")
    if i % 10 == 0:
        out.append("| " + " | ".join(f"Col {c}" for c in range(10)) + " |\n|" + "---|" * 10 + "\n")
        for r in range(5):
            out.append("| " + " | ".join(f"cell {r}.{c} with some longer wrapping text" for c in range(10)) + " |\n")
        out.append("\n")
    i += 1
sys.stdout.write("".join(out))
