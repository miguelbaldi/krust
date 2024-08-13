use adw::prelude::*;


pub(crate) fn show_error_alert(parent: &impl IsA<gtk::Widget>, message: String) {
    let alert = adw::AlertDialog::builder()
    .heading_use_markup(true)
            .heading("<span foreground='red'><b>Error</b></span>")
            .title("Error")
            .body(message.as_str())
            .close_response("close")
            .default_response("close")
            .can_close(true)
            .receives_default(true)
            .build();
        alert.add_response("close", "Ok");
        alert.set_response_appearance("close", adw::ResponseAppearance::Destructive);
        alert.present(parent);
}
