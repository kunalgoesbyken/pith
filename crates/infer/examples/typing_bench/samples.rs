pub struct Sample {
    pub name: &'static str,
    pub title: &'static str,
    pub text: String,
    pub start: usize,
    pub corrections: bool,
    pub suffix: &'static str,
}

const WORK_EMAIL: &str = "Hi Sarah, thanks for sending the revised contract. I reviewed the payment terms and they look good to me. Could you send over the final version so we can sign it by Friday? I will also loop in Daniel from finance so he can confirm the invoice schedule before we commit to anything.";

const CHAT: &str = "hey, are you free tonight? we were thinking of grabbing dinner around eight, maybe that new ramen place downtown. mike said he might bring his sister too so let me know how many seats to ask for. also dont forget to bring the charger you borrowed lol";

const SLACK: &str = "Hey team, quick update on the landing page. The new hero section is live on staging and I would love your feedback before we ship it tomorrow morning. The only open item is the mobile layout, which still wraps badly on small screens.";

const GERMAN: &str = "Hallo Herr Schmidt, vielen Dank für Ihre Nachricht. Ich habe den Vertrag geprüft und habe noch zwei Fragen zu den Zahlungsbedingungen. Könnten wir uns morgen um zehn Uhr kurz telefonisch abstimmen? Ich bin den ganzen Vormittag im Büro erreichbar.";

const SPANISH: &str = "Hola Marta, muchas gracias por tu mensaje. Te confirmo que la reunión será el jueves a las diez de la mañana en la oficina principal. Si necesitas cambiar la hora, avísame con tiempo para reservar otra sala.";

const HINDI: &str = "नमस्ते भैया, मैं कल सुबह दस बजे आपके घर आऊँगा। कृपया दस्तावेज़ तैयार रखना। अगर कोई समस्या हो तो मुझे फ़ोन कर देना, मैं तुरंत बता दूँगा कि कितनी देर में पहुँच सकता हूँ।";

const JAPANESE: &str = "田中さん、お疲れ様です。先日お送りした見積書についてご確認いただけましたでしょうか。ご不明な点がございましたら、いつでもご連絡ください。来週の月曜日までにご返信をいただければ幸いです。よろしくお願いいたします。";

const CHINESE: &str = "王经理，您好。感谢您上周的来信。关于下个季度的合作方案，我们已经和团队讨论过，大家都觉得很有价值。请您在周五之前确认具体的时间安排，以便我们提前准备相关材料。谢谢！";

const BULLETS: &str = "Agenda for Monday:\n- Review the Q3 budget numbers\n- Finalize the hiring plan for the support team\n- Decide on the vendor for the new CRM\n- Prepare slides for the board meeting on Thursday\n- Send the follow up notes to everyone\n\nOpen questions:\n- Who owns the onboarding docs?\n- Do we still need the weekly status call?\n- When does the new office lease start?\n";

const NUMBERS: &str = "Version 3.5 is out. See www.example.com/docs for details, e.g. the install guide at https://example.com/start. The price is $19.99 per month, i.e. about 1.5% less than v3.4. Dr. Smith said the U.S. release is due on 12/05/2026 at 9:30 a.m. We shipped 1,250 units (approx. 3.2 kg each), etc. Call 555-0142 or email support@example.com for help.";

const CORRECTIONS: &str = "I am writing to let you know that I will be out of the office next week. Please reach out to Daniel if you need anything urgent while I am away, and he will make sure the right person gets back to you as soon as possible.";

const MIDLINE_CLOSING: &str = "Thanks for the quick turnaround on the revised contract (I reviewed the payment terms and they look good to me, but could you send over the final version so we can sign it by Friday";

const MIDLINE_NUMBERS: &str = "Please review the pricing (the plan costs 19.99 dollars per month, which is about 1.5 percent less than version 3.4, and the full details are at www.example.com/docs";

const LONG_PARAGRAPHS: [&str; 7] = [
    "Project overview. The goal of this project is to replace the aging order processing pipeline with a service that is easier to operate and cheaper to run. Today every order passes through a monolithic batch job that runs once per hour, which means customers wait up to sixty minutes for a confirmation email and the support team cannot see the state of an order until the next batch completes.",
    "Current problems. The batch job is hard to test because it talks to the database, the payment provider and the warehouse system in a single transaction. When any of these is slow, the whole run slows down, and when one fails the job retries everything from the start. Last quarter this caused two incidents where customers were charged twice. The on call engineers also need to know the exact order in which the steps run, which is documented only in a wiki page that has not been updated in two years.",
    "Proposed design. We will split the pipeline into small independent stages connected by a durable queue. Each stage reads one message, does one thing, and writes the result back to the queue or to the database. The payment stage becomes idempotent by storing the payment reference next to the order identifier, so a retry can never create a second charge. The warehouse stage is allowed to fail and retry on its own schedule without blocking confirmation emails, which can be sent as soon as payment succeeds.",
    "Migration plan. We will run the new pipeline in shadow mode for two weeks. During this time every order is processed by both systems, but only the old system sends emails and talks to the warehouse. A comparison job checks that both pipelines reach the same final state for every order and reports any difference to the team channel. Once the difference rate stays below one in ten thousand for a full week, we switch ten percent of traffic to the new pipeline and increase it every few days.",
    "Risks. The largest risk is that the queue becomes a single point of failure. We mitigate this by running it across three zones and by keeping the old batch job available as a fallback for at least one quarter after the cutover. A second risk is that stages may process messages out of order, so every message carries a version number and stages ignore anything older than what they have already seen. A third risk is the limited time of the platform team, which is also responsible for the database upgrade planned for the same month.",
    "Open questions. We still need to decide how long failed messages stay in the dead letter queue before they are deleted, and who is responsible for reviewing them each morning. We also need an agreement with the finance team about how refunds are represented, because the current system stores them as negative orders and the new design treats them as separate events. Finally, the support team has asked for a search page that shows the full history of an order, which we have not yet estimated.",
    "Next steps. This week we will finish the prototype of the payment stage and measure how many messages per second it can handle on the smallest instance size. Next week we will review the results with the platform team and decide whether the queue should be managed by us or by the cloud provider. After that we will write the detailed rollout checklist and schedule the shadow run with the support lead so that nobody is surprised.",
];

fn plain(name: &'static str, title: &'static str, text: &str) -> Sample {
    Sample {
        name,
        title,
        text: text.to_string(),
        start: 12,
        corrections: false,
        suffix: "",
    }
}

pub fn all() -> Vec<Sample> {
    let document = LONG_PARAGRAPHS.join("\n\n");
    assert!(document.chars().count() > 3000);
    let start = document.chars().count() * 3 / 4;
    vec![
        plain("work-email", "Re: Q3 vendor contract - Gmail", WORK_EMAIL),
        plain("casual-chat", "Messages - WhatsApp", CHAT),
        plain("slack", "#design-team | Acme - Slack", SLACK),
        plain("german", "Antwort - Outlook", GERMAN),
        plain("spanish", "Respuesta - Gmail", SPANISH),
        plain("hindi", "संदेश - WhatsApp", HINDI),
        plain("japanese", "返信 - Gmail", JAPANESE),
        plain("chinese", "回复 - 微信", CHINESE),
        plain("bullets", "Weekly sync notes - Notion", BULLETS),
        plain("numbers-urls", "Release notes - Notion", NUMBERS),
        Sample {
            suffix: ").",
            ..plain("midline-close", "Re: contract - Gmail", MIDLINE_CLOSING)
        },
        Sample {
            suffix: ").",
            ..plain("midline-numbers", "Re: pricing - Gmail", MIDLINE_NUMBERS)
        },
        Sample {
            corrections: true,
            ..plain("corrections", "Untitled - Notepad", CORRECTIONS)
        },
        Sample {
            start,
            ..plain(
                "long-document",
                "Order pipeline design - Google Docs",
                &document,
            )
        },
    ]
}
