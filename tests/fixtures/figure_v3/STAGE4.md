# Stage 4 snapshot contract

Graphics acceptance is deferred to the orchestrator. The .m recipes create
actual Octave objects; no generated stage 4 JSON is committed as measurement.

Unfilled `__contour__` groups are flattened only after `fill=off`, `showtext=off`
and visible-child checks. Octave's resolved edge-only patch children become
`kind=line, role=contour`, with scalar `level`, `contour_closed`, original XY
and optional Z arrays. Finite polygon closure is explicit; a terminal missing
vertex preserves an open path. Closure's repeated endpoint uses its path offset
as source index. No contour algorithm runs in the browser. `contour_color` is a
constant/none descriptor or flat indexed scalar descriptor, association `level`.
CLim/colormap retain the colours established by Octave's LevelList listeners.
Each path is bounded at 2,000 samples including the conservative closure slot;
paths are never decimated. Filled and labelled groups reject the whole figure
with `contour_filled` and `contour_labels`. Ordinary line children require an
explicit scalar level; no level is guessed from their Z position.
`clabel(C)` creates axes-level text without provenance metadata, so any visible
free text alongside a contour is conservatively rejected as `contour_labels`.
Axes labels/titles and legends do not count as free text for this boundary.

`patch3d` has finite column-major `[N,3]` `vertices`, NaN-padded `[F,K]` `faces`,
`index_base=0`, `triangulation=first-valid-ear-v1`, `polygon_vertex_limit=64`,
`triangle_indices=[T,3]`, `triangle_faces=[T]`, `edge_indices=[E,3]` (start,
end, source face). All indices are zero based, padding is null in JSON.
The source faces remain present; the validator recomputes connectivity and
requires exactly matching triangles and edges.

A repeated final closing index is removed before triangulation. Each polygon
must be planar and simple with 3–64 distinct vertices. Normalize per face before
predicates; tolerance is 1e-10. Reject nonplanarity, duplicate points,
self-intersection, overlapping boundary edges and ambiguous degeneracy. Project
onto the plane orthogonal to the largest normal component. Scan remaining
vertices in source order; remove the first strictly convex ear containing no
other remaining vertex (including its boundary); restart the scan after every
removal. Preserve winding. This fixed rule handles convex and concave faces.
Edges contain only source polygon boundary segments, including closure; ear
diagonals are never drawn. Existing figure/object/JSON budgets still apply:
40,000 patch vertices, 80,000 triangles, 100,000 figure vertices and 200,000
figure triangles. Rectangular face padding participates in conservative
preflight upper bounds.

Face/edge descriptors support none, constant RGB, or flat indexed/truecolor
scalar or per-face values. Flat descriptor association is `face`; truecolor
is `[F,1,3]`, indexed `[F,1]`; one row broadcasts. Indexed scaled/direct mapping
uses existing colour utilities including integer indexing. Truecolor is unit
single/double RGB. Per-vertex arrays, ambiguous equal face/vertex counts, interp,
textures, nonunit alpha, effective lighting and over-budget objects retain PNG.
Actual Octave patch enums do not accept texturemap; the protocol rejects forged
texture descriptors in unit tests.

Picking snaps a face hit to the nearest original source face vertex (ties:
smallest source vertex index). Hit depth remains the actual triangle depth for
occlusion. Boundary vertices of edge-only patches are pickable. Returned
`face`, `vertex`, XY(Z) and raw flat colour values reference source data, never
uploaded Float32 values. Face/Vertex/Level tips are localized. Face colours and
the existing edge contrast policy remain unchanged.
