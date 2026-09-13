CREATE TABLE record_index_generation(client TEXT NOT NULL, project TEXT NOT NULL, generation INTEGER NOT NULL, PRIMARY KEY(client,project));
CREATE TRIGGER record_index_insert AFTER INSERT ON records BEGIN
  INSERT INTO record_index_generation VALUES(NEW.client,NEW.project,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1;
END;
CREATE TRIGGER record_index_update AFTER UPDATE ON records BEGIN
  INSERT INTO record_index_generation VALUES(NEW.client,NEW.project,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1;
END;
CREATE TRIGGER record_index_delete AFTER DELETE ON records BEGIN
  INSERT INTO record_index_generation VALUES(OLD.client,OLD.project,1) ON CONFLICT(client,project) DO UPDATE SET generation=generation+1;
END;
PRAGMA user_version=20;
