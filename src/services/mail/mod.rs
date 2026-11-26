//! 邮件投递、队列、审计和模板服务。

/// Product mail audit entries for sends and delivery failures.
pub mod audit;

/// Outbox queue: enqueue rendered payloads, dispatch due mail, drain the queue.
pub mod outbox;

/// Delivery entry points: runtime SMTP sender and immediate rendered sends.
pub mod sender;

/// Template payloads, placeholder sets and stored-payload rendering.
pub mod template;
