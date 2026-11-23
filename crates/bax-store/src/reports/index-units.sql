-- 每次只处理一份战报。转入的实例仍归原出兵者，AI 的转交也参与归属判断。
WITH players AS MATERIALIZED (
    SELECT p.key AS owner, p.value AS data, COALESCE(p.value ->> '$.Id', CAST(p.key AS INTEGER)) AS id
    FROM json_each(CASE WHEN json_valid(?3) THEN ?3 END, '$.matchInfo.Data') p
    WHERE p.type = 'object'
), transfers AS MATERIALIZED (
    SELECT p.owner, t.key AS instance, t.value ->> '$.Id' AS unit
    FROM players p, json_each(p.data, '$.TransferredUnits') t WHERE t.type = 'object'
), deployed AS (
    SELECT p.owner, u.value ->> '$.Id' AS unit
    FROM players p, json_each(p.data, '$.UnitData') u
    WHERE u.type = 'object' AND NOT EXISTS (
        SELECT 1 FROM transfers t WHERE t.instance = u.key AND t.owner <> p.owner
    )
    UNION
    SELECT owner, unit FROM transfers
), units AS (
    SELECT owner, json_group_array(unit) AS ids FROM deployed
    WHERE typeof(unit) = 'integer' GROUP BY owner
)
INSERT INTO report_player_units(match_id, player_id, start_time, unit_ids)
SELECT ?1, p.id, ?2, COALESCE(u.ids, '[]') FROM players p LEFT JOIN units u USING (owner)
WHERE typeof(p.id) = 'integer' AND p.id >= 0
  AND (p.data ->> '$.Id' IS NOT NULL OR CAST(p.id AS TEXT) = trim(p.owner))
ON CONFLICT(match_id, player_id) DO NOTHING
