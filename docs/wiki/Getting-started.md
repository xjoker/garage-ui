# Getting started

Choose either [connect to an existing Garage](Deployment.md#connect-to-an-existing-garage) or [run Garage and the UI together](Deployment.md#run-garage-and-the-ui-together). Both use ready-made containers and work without cloning the repository. The [README quick start](https://github.com/xjoker/garage-ui/blob/dev/README.md#quick-start-docker-compose) has the shortest instructions.

For an existing cluster, you need its Admin API URL and token, S3 API URL, and region. Garage v2.3.0 or newer is required for writes; compatibility has been tested with v2.3.0 and v2.4.1. The combined stack supplies these settings automatically.

Open **[http://localhost:3909](http://localhost:3909)** on the Docker host, or use the [SSH tunnel](Deployment.md#serve-the-ui-over-https) for a remote host. The UI has its own accounts; a Garage token or S3 key is not a UI login.

## Create the first administrator

1. On the setup screen, enter a username and password.
2. Choose **Create administrator**.
3. When signed in, open **Overview** and check cluster health. Open **Settings** to change your own password or appearance.

Usernames are 1–64 characters, without spaces, control characters, or semicolons. ASCII username case is ignored for uniqueness. Passwords need at least 10 characters, no more than 72 UTF-8 bytes, and cannot be blank. Use a long, unique password.

Setup is available only while the UI database contains no users. To pre-create an account or recover access, see [Users and authentication](Users-and-authentication.md#recover-access-from-the-command-line).

## Make the first storage operation

1. Open **Buckets**, choose **Create**, enter a global alias, and choose **Create bucket**.
2. Open **Keys**, create an S3 access key, and save its secret securely.
3. Open the bucket's **Permissions** tab, choose the key, and grant the required **Read**, **Write**, and **Owner** permissions.
4. Open the bucket's **Objects** tab. If prompted for **Bucket key**, select an eligible key and choose **Use this key**.
5. Choose a small file in **Choose files**. Wait for **Done**, then select the object to inspect its metadata or download it.

If the list fails, check [Configuration](Configuration.md#connect-object-operations) and [Troubleshooting](Troubleshooting.md). A working admin API alone does not establish that object operations are configured.
