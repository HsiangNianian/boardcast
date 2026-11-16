# Interface translations

Each `.ftl` file is the interface in one language, in [Fluent](https://projectfluent.org/fluent/guide/).
`en.ftl` is the source: every message starts there, and any message a language lacks is shown in English.

```ftl
saved-to-path = Saved to { $path }
line-count = { $n ->
    [one] { $n } line
   *[other] { $n } lines
}
```

- Keep the message IDs and the `$variables` as they are; put the variables wherever your language needs them.
- Counts use your language's plural categories (`one`, `few`, `many`, `other`, as [CLDR](https://www.unicode.org/cldr/charts/latest/supplemental/language_plural_rules.html) lists them). A language without plurals writes the plain text.
- A literal brace is written `{"{"}`, and space at either end of a text `{" "}`.

To improve a translation, edit its file. To add a language, copy `en.ftl` to `<code>.ftl`, translate it, and list it in
`LANGUAGES` and `SOURCES` in `crates/ui/src/i18n.rs`. `cargo test -p boardcast-ui i18n` checks that every file reads,
uses only the variables English gives, and that every message the code names exists.
