-- 每次只处理一份战报。转入的实例仍归原出兵者，AI 的转交也参与归属判断。
WITH players AS MATERIALIZED (
    SELECT p.key AS owner, p.value AS data, COALESCE(p.value ->> '$.Id', CAST(p.key AS INTEGER)) AS id
    FROM json_each(CASE WHEN json_valid(?3) THEN ?3 END, '$.matchInfo.Data') p
    WHERE p.type = 'object'
), transfers AS MATERIALIZED (
    SELECT p.owner, t.key AS instance, t.value ->> '$.Id' AS unit, t.value ->> '$.SpawnTime' AS spawned
    FROM players p, json_each(p.data, '$.TransferredUnits') t WHERE t.type = 'object'
), deployed AS (
    SELECT p.owner, u.key AS instance, u.value ->> '$.Id' AS unit, u.value ->> '$.SpawnTime' AS spawned
    FROM players p, json_each(p.data, '$.UnitData') u
    WHERE u.type = 'object' AND NOT EXISTS (
        SELECT 1 FROM transfers t WHERE t.instance = u.key AND t.owner <> p.owner
    )
    UNION
    SELECT owner, instance, unit, spawned FROM transfers
), instances AS (
    SELECT owner, instance, unit, MIN(spawned) AS spawned FROM deployed
    WHERE typeof(unit) = 'integer' GROUP BY owner, instance, unit
), types AS (
    SELECT owner, unit, COUNT(*) AS n, MIN(spawned) AS first_spawn FROM instances GROUP BY owner, unit
), units AS (
    SELECT owner, json_group_array(unit) AS ids,
           json_group_array(json_array(unit, n, first_spawn)) AS deployment FROM types GROUP BY owner
)
INSERT INTO report_player_units(match_id, player_id, start_time, unit_ids, team_id, rating_delta, deployment,
                              report_name, kills, losses, objectives, supplied_allies)
SELECT ?1, p.id, ?2, COALESCE(u.ids, '[]'),
       CASE WHEN json_type(p.data, '$.TeamId') IS NULL THEN 0
            WHEN json_type(p.data, '$.TeamId') = 'integer' THEN p.data ->> '$.TeamId' END,
       CASE WHEN json_type(p.data, '$.OldRating') IN ('real','integer')
              AND json_type(p.data, '$.NewRating') IN ('real','integer')
            THEN (p.data ->> '$.NewRating') - (p.data ->> '$.OldRating') END, COALESCE(u.deployment, '[]'),
       NULLIF(TRIM(p.data ->> '$.Name'), ''), COALESCE(p.data ->> '$.Destruction', 0),
       COALESCE(p.data ->> '$.Losses', 0), COALESCE(p.data ->> '$.ObjectivesCaptured', 0),
       COALESCE(p.data ->> '$.SupplyPointsConsumedByAllies', 0)
FROM players p LEFT JOIN units u USING (owner)
WHERE typeof(p.id) = 'integer' AND p.id >= 0
  AND (p.data ->> '$.Id' IS NOT NULL OR CAST(p.id AS TEXT) = trim(p.owner))
ON CONFLICT(match_id, player_id) DO UPDATE SET team_id = excluded.team_id,
    rating_delta = excluded.rating_delta, deployment = excluded.deployment, report_name = excluded.report_name,
    kills = excluded.kills, losses = excluded.losses, objectives = excluded.objectives, supplied_allies = excluded.supplied_allies
