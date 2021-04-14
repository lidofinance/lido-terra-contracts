if [ ! -d "./localterra" ]; then
  echo "Cloning localterra..."
  git clone https://www.github.com/terra-project/LocalTerra
  mv LocalTerra localterra
fi

echo "Running local testnet (clean up first)..."
cd localterra || exit
docker-compose down
docker-compose rm
docker-compose up
