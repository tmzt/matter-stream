// tufte_demo.tsx — High-density TSX document showcasing Tufte principles
// This is the test fixture compiled by the mtd1 engine.

import { TufteCard, Story, Spreadsheet, Path } from "matter-stream";

export default function TufteDemo() {
  return (
    <TufteCard x={20} y={10} width={600}>
      {/* Dense, line-wrapped text paragraph */}
      <Story token={{ wordIndex: 1, id: 1001 }}>
        The visual display of quantitative information demands that we give
        the viewer the greatest number of ideas in the shortest time with
        the least ink in the smallest space. Data graphics should draw
        attention to the substance rather than to methodology, graphic
        design, or technology of graphic production.
      </Story>

      {/* Data table with zebra striping and column alignment */}
      <Spreadsheet
        zebra={true}
        colWidths={[120, 100, 100, 120]}
        headers={["Quarter", "Revenue", "Growth", "Margin"]}
        rows={[
          ["Q1 2024", "$12.4M", "+8.2%", "34.1%"],
          ["Q2 2024", "$13.1M", "+5.6%", "35.8%"],
          ["Q3 2024", "$14.8M", "+13.0%", "36.2%"],
          ["Q4 2024", "$15.2M", "+2.7%", "37.0%"],
          ["Q1 2025", "$16.9M", "+11.2%", "38.4%"],
          ["Q2 2025", "$18.3M", "+8.3%", "39.1%"],
        ]}
      />

      {/* Inline sparkline compiled to DRAW_SHAPE primitives */}
      <Path
        segments={[
          [2, 30],
          [4, 30],
          [3, 30],
          [7, 30],
          [5, 30],
          [8, 30],
          [6, 30],
          [10, 30],
          [9, 30],
          [12, 30],
          [11, 30],
          [14, 30],
        ]}
      />
    </TufteCard>
  );
}
