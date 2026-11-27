-- 只在归档、替换或升级时读取大段原文。接口会省略值为 0 的 Type。
INSERT OR REPLACE INTO report_modes(match_id, raw_mode)
SELECT match_id, CASE
    WHEN NOT json_valid(raw) THEN 'unknown'
    WHEN json_type(raw, '$.matchInfo.Data') IS NOT 'object' THEN 'unknown'
    WHEN EXISTS (SELECT 1 FROM json_each(raw, '$.matchInfo.Data') p
                 WHERE p.type = 'object' AND COALESCE(p.value ->> '$.Id', CAST(p.key AS INTEGER)) < 0) THEN 'pve'
    WHEN raw ->> '$.matchInfo.Type' = 1 OR EXISTS (
        SELECT 1 FROM json_each(raw, '$.matchInfo.Data') p
        WHERE p.type = 'object' AND p.value ->> '$.TeamId' = 100) THEN 'custom'
    WHEN COALESCE(raw ->> '$.matchInfo.Type', 0) = 0
         AND EXISTS (SELECT 1 FROM json_each(raw, '$.matchInfo.Data') WHERE type = 'object') THEN 'ranked'
    ELSE 'unknown' END
FROM match_reports
