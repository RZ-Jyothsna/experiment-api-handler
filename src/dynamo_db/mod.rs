use std::collections::HashMap;

use aws_sdk_dynamodb::{
  Client,
  Error,
  types::{
    builders:: {AttributeDefinitionBuilder, KeySchemaElementBuilder},
    AttributeDefinition,
    AttributeValue,
    KeySchemaElement,
    KeyType,
    ScalarAttributeType
  },
  operation::get_item::GetItemOutput
};

pub struct DynamoDB {
    client: Client,
}

#[derive(Debug, Clone)]
pub struct Attribute {
    attribute_name: String,
    attribute_type: ScalarAttributeType,
    key_type: KeyType,
}

impl DynamoDB {
    pub fn new(client: Client) -> Self {
        DynamoDB { client }
    }

    pub async fn create_table(
      self,
      table_name: &str,
      attributes: Vec<Attribute>
    ) -> Result<(), Error> {
      let mut attribute_definitions = vec!();
      let mut key_schema = vec!();

      attributes.into_iter().for_each(|attr| {
          let attribute = AttributeDefinition::builder()
            .attribute_name(attr.attribute_name.clone())
            .attribute_type(attr.attribute_type)
            .build();

          let key_ele = KeySchemaElement::builder()
            .attribute_name(attr.attribute_name)
            .key_type(attr.key_type)
            .build();

          match (attribute, key_ele) {
            (Ok(attr), Ok(key)) => {
              attribute_definitions.push(attr);
              key_schema.push(key);
            }
            _ => {
              println!("Error creating attribute or key element");
            }
          }
      });

      let response = self.client
        .create_table()
        .table_name(table_name)
        .set_attribute_definitions(Some(attribute_definitions))
        .set_key_schema(Some(key_schema))
        .send()
        .await?;

      Ok(())
    }

    pub async fn insert_item(
      self,
      table_name: &str,
      item:Option<HashMap<String, AttributeValue>>
    ) -> Result<(), Error> {
      let response = self.client
        .put_item()
        .table_name(table_name)
        .set_item(item)
        .send()
        .await?;

      Ok(())
    }

    pub async fn get_item(
      self,
      table_name: &str,
      key:Option<HashMap<String, AttributeValue>>
    ) -> Result<GetItemOutput, Error> {
      let response = self.client
        .get_item()
        .table_name(table_name)
        .set_key(key)
        .send()
        .await?;

      Ok(response)
    }

    pub async fn update_item(
      self,
      table_name: &str,
      key:Option<HashMap<String, AttributeValue>>,
      updated_vals: HashMap<String, String>,
      update_expression: String,
      attribute_names: Option<HashMap<String, String>>,
      attribute_values: Option<HashMap<String, AttributeValue>>
    ) -> Result<(), Error> {
      let response = self.client
        .update_item()
        .table_name(table_name)
        .set_key(key)
        .update_expression(update_expression)
        .set_expression_attribute_values(attribute_values)
        .set_expression_attribute_names(attribute_names)
        .send()
        .await?;

      Ok(())
    }
}