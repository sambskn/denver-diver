# denver-diver
playing around with open street map data downlaoded to pmtiles, served with martin as vector tiles, and rendered with bevy (native or web)

## prep it
Need the tileserver (martin, reccomend to install with cargo binstall) and a client, either the native one or the web app
- The native app didn't work right when kicked off from inside a devenv for me, so jsut run martin and the app separately

Should be able to do the web app on port 8080 with a simple `devenv up`