# 0006 Enemy armor and attack mix weighted by HP

Status: Accepted (merged, schema v4). Full research and sources: [enemy-weighting.md](../enemy-weighting.md).

## Context
Counting enemies per armor type treats a one-HP minion and a boss alike. A raid boss with parts (the drum
crab: special-armor body, light-armor hull and claws) cannot be one type, and equal shares over all types
tell the player nothing.

## Decision
An enemy weighs its HP at the encounter's level and grade, using SchaleDB's own formula. Summoned, Student
and DestructibleProjectile weigh 0. In a raid, a non-boss entry whose HP equals a boss's HP is that boss's own
body and weighs 0; parts with less HP keep their weight. The expected damage multiplier of a student against
the mix is the weighted mean. The UI shows shares ("경장갑 36%", under 5 percent merged into 기타) and says
"HP 비중 기준". There are no role multipliers, because no source supports a number.

## Alternatives (compared on real rows)
Head count (old) gives 78/22 for the drum crab. Plain HP weighting gives 91/9, which hides the hull. HP with
the structural mirror rule gives Normal 64/36, Insane 68/32 and Lunatic 82/18 (chosen).

## Consequences
- Fit tiers change for several stages (현상수배 고저택, KV 하이웨이, drum crab).
- HP share is not time share. Swarms killed together by area attacks are under-counted.
- Raid extra HP buffs and floor raid stat changes are not modelled.

## Evidence
`weight_report` example prints head count against HP weighting side by side.

## Revisit when
Time-to-kill data per part becomes available, or live checks show the mirror rule is wrong.
