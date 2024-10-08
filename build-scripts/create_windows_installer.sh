# Copyright (c) 2024, Miguel A. Baldi Hörlle <miguel.horlle@gmail.com>. All rights reserved. Use of
# this source code is governed by the GPL-3.0 license that can be
# found in the COPYING file.

docker rm setup
docker container create --name setup amake/innosetup setup.iss
docker cp ./package setup:/work/
docker cp data/images/krust.ico setup:/work/krust.ico
export MY_VERSION=$(git describe)
sed -i "s/MY_VERSION/$MY_VERSION/g" setup.iss
docker cp setup.iss setup:/work/
iconv -f utf8 -t iso8859-1 COPYING > LICENSE
docker cp LICENSE setup:/work/LICENSE
docker start -i -a setup
docker cp setup:/work/Output/. .
docker rm setup
zip -r package.zip package
