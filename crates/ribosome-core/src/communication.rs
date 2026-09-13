use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::{derived_split, id, now_ms, validate},
};
use rusqlite::params;
use serde_json::json;

impl Store {
    pub fn send_message(&self, run_id: &str, request: &MessageSend) -> Result<Message> {
        validate("MessageSend", &serde_json::to_value(request)?)?;
        let tx = self.write_transaction()?;
        let grant = self.require_active(run_id)?;
        let recipient = self.run_grant(&request.recipient)?;
        if recipient.id != grant.id {
            return Err(Error::denied("recipient is outside communication grant"));
        }
        let mut count = 0;
        for message in self
            .db
            .prepare(
                "SELECT id FROM messages WHERE grant_id=?1 AND acknowledged=0 AND source_format=1",
            )?
            .query_map([&grant.id], |r| r.get::<_, String>(0))?
        {
            if self.source_available(&grant, "event", &message?)? {
                count += 1;
            }
        }
        if count >= 100 {
            return Err(Error::exhausted("inbox capacity reached"));
        }
        let mut message = Message {
            id: id(),
            sender: run_id.into(),
            recipient: request.recipient.clone(),
            topic: request.topic.clone(),
            body: request.body.clone(),
            correlation: request.correlation.clone(),
            sequence: "0".into(),
            attempts: 0,
        };
        self.db.execute(
            "INSERT INTO messages(id,grant_id,sender,recipient,body,source_format) VALUES (?1,?2,?3,?4,?5,1)",
            params![
                message.id,
                grant.id,
                run_id,
                message.recipient,
                serde_json::to_string(&message)?
            ],
        )?;
        message.sequence = self.db.last_insert_rowid().to_string();
        self.db.execute(
            "UPDATE messages SET body=?2 WHERE id=?1",
            params![message.id, serde_json::to_string(&message)?],
        )?;
        self.ingest(&Event { id:message.id.clone(),scope:grant.scope.clone(),run_id:run_id.into(),producer:"communication".into(),sequence:message.sequence.clone(),kind:"message_sent".into(),timestamp_ms:now_ms().to_string(),parents:vec![],correlation:message.id.clone(),artifacts:vec![],payload:json!({"message_ref":message.id,"sender":run_id,"recipient":message.recipient,"authority":"sender-authored message; content is retained in the inbox store"}).as_object().unwrap().clone(),provenance:Provenance{origin:Origin::Observed,source_refs:self.run_context_sources(run_id)?,scenario_family:"communication".into(),split:derived_split(&grant.visible_splits),limitations:vec![]}})?;
        tx.commit()?;
        Ok(message)
    }

    pub fn inbox(&self, run_id: &str) -> Result<Inbox> {
        let tx = self.write_transaction()?;
        let grant = self.require_active(run_id)?;
        let mut stmt=self.db.prepare("SELECT sequence,body,attempts,id FROM messages WHERE recipient=?1 AND grant_id=?2 AND source_format=1 AND acknowledged=0 AND attempts<3 ORDER BY sequence")?;
        let mut messages = Vec::new();
        for row in stmt.query_map(params![run_id, grant.id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, String>(3)?,
            ))
        })? {
            let (seq, body, attempts, id) = row?;
            if !self.source_available(&grant, "event", &id)? {
                continue;
            }
            let mut message: Message = serde_json::from_str(&body)?;
            message.sequence = seq.to_string();
            message.attempts = attempts + 1;
            messages.push(message);
            if messages.len() == 50 {
                break;
            }
        }
        drop(stmt);
        for message in &messages {
            self.db.execute(
                "UPDATE messages SET attempts=attempts+1 WHERE id=?1",
                [&message.id],
            )?;
        }
        tx.commit()?;
        Ok(Inbox { messages })
    }

    pub fn acknowledge_message(&self, run_id: &str, message_id: &str) -> Result<()> {
        if self.db.execute(
            "UPDATE messages SET acknowledged=1 WHERE id=?1 AND recipient=?2",
            params![message_id, run_id],
        )? != 1
        {
            return Err(Error::missing("message not found for recipient"));
        }
        Ok(())
    }
}
