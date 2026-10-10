//! Unit tests of the admin registrations (SR-06, SR-20, SR-102).

use rstest::rstest;

use reinhardt::admin::core::AdminError;
use reinhardt::admin::{AdminSite, AdminUser, ModelAdmin};

use crate::apps::accounts::models::User;
use crate::apps::accounts::server::admin::register_model_admins;
use crate::apps::accounts::server::admin::social_account::SocialAccountAdmin;
use crate::apps::accounts::server::admin::user::UserAdmin;

fn user_with(is_active: bool, is_staff: bool) -> User {
	User::build()
		.github_user_id(1)
		.github_login("octocat".to_owned())
		.display_name("The Octocat".to_owned())
		.avatar_url(None)
		.email(None)
		.is_active(is_active)
		.is_staff(is_staff)
		.finish()
}

/// (view, add, change, delete) permissions the model admin grants `user`.
async fn permissions_of(admin: &impl ModelAdmin, user: &User) -> (bool, bool, bool, bool) {
	(
		admin.has_view_permission(user).await,
		admin.has_add_permission(user).await,
		admin.has_change_permission(user).await,
		admin.has_delete_permission(user).await,
	)
}

#[rstest]
fn sr_20_admin_cannot_change_staff_or_identity_fields() {
	// Arrange
	let admin = UserAdmin;

	// Act
	let readonly = admin.readonly_fields();

	// Assert
	for field in ["is_staff", "github_user_id", "github_login", "id"] {
		assert!(readonly.contains(&field), "`{field}` must be read-only");
	}
}

#[rstest]
#[tokio::test]
async fn sr_20_admin_never_creates_or_deletes_users() {
	// Arrange
	let admin = UserAdmin;
	let staff = user_with(true, true);

	// Act
	let permissions = permissions_of(&admin, &staff).await;

	// Assert
	assert_eq!(permissions, (true, false, true, false));
}

#[rstest]
fn sr_06_admin_exposes_no_token_column_for_social_accounts() {
	// Arrange
	let admin = SocialAccountAdmin;

	// Act
	let list = admin.list_display();
	let fields = admin.fields().unwrap_or_default();

	// Assert
	assert!(!fields.is_empty(), "the field whitelist must be explicit");
	for column in list.iter().chain(fields.iter()) {
		assert!(
			!column.starts_with("encrypted_"),
			"`{column}` must not expose token material"
		);
	}
}

#[rstest]
#[tokio::test]
async fn sr_06_admin_never_edits_social_accounts() {
	// Arrange
	let admin = SocialAccountAdmin;
	let staff = user_with(true, true);

	// Act
	let permissions = permissions_of(&admin, &staff).await;

	// Assert
	assert_eq!(permissions, (true, false, false, false));
}

#[rstest]
#[case::staff(true, true, true)]
#[case::regular_user(true, false, false)]
#[case::deactivated_staff(false, true, false)]
fn sr_20_user_reaches_the_admin_site_only_as_active_staff(
	#[case] is_active: bool,
	#[case] is_staff: bool,
	#[case] reaches_admin: bool,
) {
	// Arrange
	let user = user_with(is_active, is_staff);

	// Act
	let admin_user: &dyn AdminUser = &user;

	// Assert
	assert_eq!(
		admin_user.is_active() && admin_user.is_staff(),
		reaches_admin
	);
	assert!(!admin_user.is_superuser());
	assert_eq!(admin_user.get_username(), "octocat");
}

#[rstest]
fn sr_20_every_accounts_model_is_registered_with_the_admin_site() {
	// Arrange
	let site = AdminSite::new("Control Plane");

	// Act
	register_model_admins(&site).unwrap();

	// Assert
	let mut registered = site.registered_models();
	registered.sort();
	assert_eq!(registered, vec!["Social Account", "User"]);
	let user_admin = site.get_model_admin("User").unwrap();
	assert!(user_admin.readonly_fields().contains(&"is_staff"));
}

#[rstest]
fn sr_20_registering_twice_is_rejected() {
	// Arrange
	let site = AdminSite::new("Control Plane");
	register_model_admins(&site).unwrap();

	// Act
	let second = register_model_admins(&site);

	// Assert
	assert!(matches!(&second, Err(AdminError::ValidationError(message))
		if message == "Model 'User' is already registered (as 'User')"));
}
