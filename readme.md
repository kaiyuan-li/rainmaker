# Read Me

## Commands
Build image
```
docker build -t rainmaker_okx:waterdrop .
```
## Run image using an external json config
```
docker run -v <PATH/TO/JSON>:/app/data -it rainmaker_okx:waterdrop /usr/local/bin/as_okex /app/data/okex_config.json
```
