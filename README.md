Setup:  
1. Create new dropbox app in <https://www.dropbox.com/developers/apps>.    
2. On permissions page of application enable: **files.metadata.write**, **files.metadata.read**, **files.content.write**, **files.content.read**.  
3. Rename **clouds_viewer.config.proto** to **clouds_viewer.config**.    
4. In **clouds_viewer.config** do one of:
   - Fill **storage.tokens.token** field (use **generated access token** from settings page of dropbox app (token must be generated after setting permissions))
   - Fill fields:
     - **storage.api_key.client_id** (use **App key** from settings page of dropbox app)
     - **storage.api_key.secret** (use **App secret** from settings page of dropbox app) 
     - **storage.redirect_addresses** (**storage.redirect_addresses** and **OAuth 2 redirect URIs** of dropbox app must be the same and in the form of <http://127.0.0.1:<port>/<maybe_anything_here>> (example: <http://127.0.0.1:7080/dropbox>))
                                                                          
 