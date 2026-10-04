# Language plugin fixtures

`elixir.zip` is an OxideTerm language plugin built from
`tree-sitter-elixir` 0.3.5, crate SHA-256
`66dd064a762ed95bfc29857fa3cb7403bb1e5cb88112de0f6341b7e47284ba40`,
using Tree-sitter CLI 0.27.0. The archive includes the upstream Apache-2.0
license and source attribution. It is test data, not an installed or bundled
runtime plugin.

Rebuild it using the marketplace repository's
`scripts/build-language-plugins.mjs elixir`, then copy the resulting ZIP here.
The syntax, editor, and installer tests consume the same real package to
exercise their respective parsing, document lifecycle, and installation boundaries.

`c.zip` and `rust.zip` preserve the syntax and editor regressions after these
grammars moved out of the host. Both use Tree-sitter CLI 0.27.0 and include the
upstream MIT license and source attribution:

| Fixture | Source crate | Crate SHA-256 |
| --- | --- | --- |
| `c.zip` | `tree-sitter-c` 0.24.2 | `a9b2eb57a55fed6b00812912e730b7a275cf4fe98bfd6a5d76263d4438371728` |
| `rust.zip` | `tree-sitter-rust` 0.24.2 | `439e577dbe07423ec2582ac62c7531120dbfccfa6e5f92406f93dd271a120e45` |

Rebuild with `scripts/build-language-plugins.mjs c rust` in the marketplace
repository and copy the ZIPs here. These archives are test fixtures only;
production builds do not embed them or load a fallback native grammar.
