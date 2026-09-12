-- Run with every Ribosome owner stopped. Records remain authoritative.
BEGIN IMMEDIATE;
DELETE FROM record_search;
INSERT INTO record_search(id,content)
SELECT id,json_extract(body,'$.body') FROM records
WHERE json_extract(body,'$.retired')=0
AND NOT (kind='memory' AND CAST(coalesce(json_extract(body,'$.body.expires_ms'),'0') AS INTEGER)>0
  AND CAST(json_extract(body,'$.body.expires_ms') AS INTEGER)<=CAST((julianday('now')-2440587.5)*86400000 AS INTEGER));
COMMIT;
