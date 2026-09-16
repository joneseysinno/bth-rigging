// BTH Rigging — project calc package (US Letter 8.5 × 11)
#import sys: inputs

#let pkg = json(bytes(inputs.data))

#set page(
  paper: "us-letter",
  margin: (x: 0.6in, y: 0.65in),
  numbering: "1",
  header: context [
    #set text(size: 8pt, fill: rgb("#78716c"))
    #grid(
      columns: (1fr, 1fr),
      align(left)[BTH Rigging · Calc package],
      align(right)[#pkg.project · #pkg.printed_at],
    )
    #v(-0.35em)
    #line(length: 100%, stroke: 0.5pt + rgb("#d6d0c6"))
  ],
  footer: context [
    #line(length: 100%, stroke: 0.5pt + rgb("#d6d0c6"))
    #v(0.25em)
    #set text(size: 7.5pt, fill: rgb("#78716c"))
    #grid(
      columns: (1fr, auto),
      align(left)[
        Calculation aid only. Manufacturer tags, ASME B30.9 / B30.26, geotechnical values,
        and a qualified person govern the lift and crane setup.
      ],
      align(right + horizon)[
        Page #counter(page).display()
      ],
    )
  ],
)

#set text(font: "Libertinus Serif", size: 9.5pt, fill: rgb("#1c1917"))
#set par(leading: 0.45em)

#let stamp(kind, label) = {
  let (bg, fg) = if kind == "over" {
    (rgb("#ffe4e6"), rgb("#9f1239"))
  } else if kind == "warn" {
    (rgb("#fef3c7"), rgb("#92400e"))
  } else {
    (rgb("#dcfce7"), rgb("#166534"))
  }
  box(
    fill: bg,
    inset: (x: 5pt, y: 2pt),
    radius: 2pt,
    text(size: 7.5pt, weight: "bold", fill: fg, label),
  )
}

#let sheet-title(title) = {
  text(size: 8pt, weight: "bold", fill: rgb("#b45309"), tracking: 0.08em)[BTH RIGGING]
  v(0.15em)
  text(size: 16pt, weight: "bold")[#title]
  v(0.4em)
}

#let meta-block(entries) = {
  set text(size: 9pt)
  for (k, v) in entries [
    *#k:* #v \
  ]
  v(0.5em)
}

#let lift-sheet(pick) = {
  sheet-title[Lift Report]
  let entries = (
    ("Project", pkg.project),
    ("Pick", pick.name),
    ("Payload", pick.payload),
    ("Rigging weight", pick.rigging_weight),
  )
  if pick.rigging_height != none {
    entries.push(("Rigging height", pick.rigging_height))
  }
  entries.push(("Hook load", pick.hook_load))
  meta-block(entries)

  if pick.warn != none {
    block(
      width: 100%,
      fill: rgb("#fef3c7"),
      inset: 8pt,
      radius: 3pt,
      text(fill: rgb("#92400e"))[#pick.warn],
    )
  } else {
    set text(size: 8pt)
    table(
      columns: (auto, 1.3fr, auto, auto, auto, auto, auto, 1.3fr, auto),
      inset: (x: 4pt, y: 4pt),
      stroke: 0.4pt + rgb("#d6d0c6"),
      fill: (_, y) => if y == 0 { rgb("#ebe4d8") } else { none },
      align: (col, _) => if col == 0 or col == 8 { center } else if col >= 2 and col <= 6 { right } else { left },
      table.header(
        [*Layer*], [*Config*], [*Rigging wt*], [*Carried*], [*Tension / leg*], [*Sling WLL*], [*Util.*], [*Hardware*], [*Status*],
      ),
      ..for row in pick.rows {
        let cfg-main = if row.is_spreader {
          text(fill: rgb("#78716c"))[#row.config]
        } else {
          [#row.config]
        }
        let cfg = [#cfg-main#for line in row.geometry [#linebreak()#text(size: 7pt, fill: rgb("#78716c"), line)]]
        let hw = if row.hardware.len() == 0 {
          text(fill: rgb("#78716c"))[—]
        } else {
          for (i, line) in row.hardware.enumerate() {
            if i > 0 { linebreak() }
            [#line]
          }
        }
        let rig = for (i, line) in row.rigging.enumerate() {
          if i > 0 { linebreak() }
          if i == 0 and not row.is_spreader { strong(line) } else { text(size: 7pt, fill: rgb("#78716c"), line) }
        }
        let carried = if row.top == "" {
          [#row.carried]
        } else {
          [#row.carried #linebreak() #text(size: 7pt, fill: rgb("#78716c"), row.top)]
        }
        (
          [#row.layer],
          cfg,
          rig,
          carried,
          [#row.tension],
          [#row.sling_wll],
          [#row.util],
          hw,
          stamp(row.status_kind, row.status),
        )
      }
    )
  }

  v(0.6em)
  set text(size: 7.5pt, fill: rgb("#78716c"))
  [Calculation aid only. Manufacturer identification tags, ASME B30.9 / B30.26, and a qualified person govern the lift. Always verify spreader and shackle tags. Rigging self-weight (slings by length, shackles, spreader bars, other tare) accumulates from the payload up to the hook; sling and shackle weights are representative catalog values — verify with the manufacturer.]
}

#let mat-sheet(analysis) = {
  sheet-title[Mat Bearing Report]
  meta-block((
    ("Project", pkg.project),
    ("Analysis", analysis.name),
    ("Outrigger", analysis.outrigger),
    ("Allowable GBP", analysis.allowable),
  ))

  if analysis.warn != none {
    block(
      width: 100%,
      fill: rgb("#fef3c7"),
      inset: 8pt,
      radius: 3pt,
      text(fill: rgb("#92400e"))[#analysis.warn],
    )
  } else {
    set text(size: 9pt)
    table(
      columns: (1.4in, 1fr, auto),
      inset: (x: 6pt, y: 5pt),
      stroke: 0.4pt + rgb("#d6d0c6"),
      fill: (_, y) => if calc.rem(y, 2) == 0 { rgb("#f7f3ec") } else { none },
      align: (col, _) => if col == 0 { left } else if col == 2 { center } else { left },
      ..for row in analysis.rows {
        let status-cell = if row.status != none and row.status_kind != none {
          stamp(row.status_kind, row.status)
        } else {
          []
        }
        (
          strong[#row.label],
          [#row.value],
          status-cell,
        )
      }
    )
  }

  v(0.6em)
  set text(size: 7.5pt, fill: rgb("#78716c"))
  [Calculation aid only. Effective length uses Duerr (2010) soil-bearing method. Manufacturer allowable outrigger load and identification tags govern mat capacity. Site geotechnical values and a qualified person govern crane setup.]
}

#if pkg.picks.len() + pkg.mats.len() == 0 [
  #sheet-title[Calc package]
  No picks or mat analyses in this project.
] else {
  for (i, pick) in pkg.picks.enumerate() {
    if i > 0 { pagebreak() }
    lift-sheet(pick)
  }
  for (i, analysis) in pkg.mats.enumerate() {
    if pkg.picks.len() + i > 0 { pagebreak() }
    mat-sheet(analysis)
  }
}
