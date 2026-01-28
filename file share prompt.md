it is time to start implementing file transfer between clients using the torrent protocol. the goal is to use the server as a private tracker communicated to over the existing quic connection. then the clients can share files among themselves. files should only persist for as long as a client that holds that file is still connected to the server and clients should delete their local seed copies when closing or disconnecting.

I imagine the workflow going a bit like this:

- a user presses a "share file" button in the UI and selects a file
- the client creates a torrent for that file and registers it with the server over quic
- a chat message is sent to all users in the same room indicating that a file is available for download.
- the chat message renders a few buttons: "download", "download and save as..."
  - pressing either button starts the torrent download using the server as a tracker
  - the files downloads to a temporary location managed by the application
  - if "download and save as..." is pressed, the file is copied to the user-specified location once the download is complete
- some types of files (e.g. images, videos) appear inline in the chat window once downloaded
- other file types appear as clickable links that open the file in the default application when clicked
- files are deleted from the temporary location when the client disconnects or closes. this cleans up any files the user downloaded but did not explicitly save.

My current thought is to use parts of `librqbit` for the client and crates from the `torrust-tracker` project for the server-side tracker functionality. I have cloned both to the vendor directory for exploration. look through their APIs, and my own codebase, and suggest how to best design and implement this feature. then we will discuss.