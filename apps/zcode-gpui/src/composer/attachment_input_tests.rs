use crate::composer::{attachment::AttachmentRef, input::Composer};
use gpui::{AppContext, TestAppContext};

fn reference(path: &std::path::Path) -> AttachmentRef {
    AttachmentRef {
        reference: path.to_string_lossy().into_owned(),
        file_name: "image.png".into(),
        mime: "image/png".into(),
        bytes: 1,
        preview_ref: None,
    }
}

#[gpui::test]
fn recovered_and_picked_refs_never_gain_deletion_ownership(cx: &mut TestAppContext) {
    let path = crate::shared::temp_attachments::write_temp_image(b"reference only", "png").unwrap();
    let composer = cx.new(Composer::new);
    composer.update(cx, |c, cx| {
        c.add_attachment(reference(&path), cx);
        assert!(c.temp_owned.is_empty());
        c.remove_attachment(0, cx);
        assert!(path.exists());
    });
    crate::shared::temp_attachments::delete_owned(&path);
}

#[gpui::test]
fn producer_ownership_survives_failed_enqueue_and_transfers_once(cx: &mut TestAppContext) {
    let path = crate::shared::temp_attachments::write_temp_image(b"unsent owner", "png").unwrap();
    let composer = cx.new(Composer::new);
    composer.update(cx, |c, cx| {
        c.add_owned_attachment(reference(&path), cx);
        c.set_text("unsent");
        let (text, attachments, owned) = c.take_submission();
        assert_eq!(owned, std::slice::from_ref(&path));
        c.restore_submission(text, attachments, owned);
        assert_eq!(c.temp_owned, std::slice::from_ref(&path));
        let (_, _, owned) = c.take_submission();
        assert_eq!(owned, std::slice::from_ref(&path));
        assert!(c.temp_owned.is_empty());
    });
    crate::shared::temp_attachments::delete_owned(&path);
}
