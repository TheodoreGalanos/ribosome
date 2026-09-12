use crate::{
    contracts::*,
    error::{Error, Result},
    store::Store,
    validation::id,
};
use rusqlite::params;

impl Store {
    pub fn send_message(&self, run_id: &str, request: &MessageSend) -> Result<Message> {
        let grant = self.require_active(run_id)?;
        let recipient = self.run_grant(&request.recipient)?;
        if recipient.id != grant.id {
            return Err(Error::denied("recipient is outside communication grant"));
        }
        let count: u32 = self.db.query_row(
            "SELECT count(*) FROM messages WHERE grant_id=?1 AND acknowledged=0",
            [&grant.id],
            |r| r.get(0),
        )?;
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
            "INSERT INTO messages(id,grant_id,sender,recipient,body) VALUES (?1,?2,?3,?4,?5)",
            params![
                message.id,
                grant.id,
                run_id,
                message.recipient,
                serde_json::to_string(&message)?
            ],
        )?;
        message.sequence = self.db.last_insert_rowid().to_string();
        Ok(message)
    }

    pub fn inbox(&self, run_id: &str) -> Result<Inbox> {
        self.require_active(run_id)?;
        let mut stmt=self.db.prepare("SELECT sequence,body,attempts FROM messages WHERE recipient=?1 AND acknowledged=0 AND attempts<3 ORDER BY sequence LIMIT 50")?;
        let mut messages = Vec::new();
        for row in stmt.query_map([run_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u32>(2)?,
            ))
        })? {
            let (seq, body, attempts) = row?;
            let mut message: Message = serde_json::from_str(&body)?;
            message.sequence = seq.to_string();
            message.attempts = attempts + 1;
            messages.push(message);
        }
        for message in &messages {
            self.db.execute(
                "UPDATE messages SET attempts=attempts+1 WHERE id=?1",
                [&message.id],
            )?;
        }
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
