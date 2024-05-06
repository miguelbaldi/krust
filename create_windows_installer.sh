docker rm setup
docker container create --name setup amake/innosetup setup.iss
docker cp ./package setup:/work/
docker cp data/images/krust.ico setup:/work/krust.ico
docker cp setup.iss setup:/work/
iconv -f utf8 -t iso8859-1 LICENSE.md > LICENSE-win.md
docker cp LICENSE-win.md setup:/work/LICENSE.md
docker start -i -a setup
docker cp setup:/work/Output/. .
docker rm setup
zip -r package.zip package
