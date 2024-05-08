
fn show_error_alert() {
    let alert = adw::AlertDialog::builder()
            .heading("Error")
            .title("Error")
            .body("Sorry, no donuts for you\nUnder construction!")
            .close_response("close")
            .default_response("close")
            .can_close(true)
            .receives_default(true)
            .build();
        alert.add_response("close", "Cancel");
        alert.present(root);
}
